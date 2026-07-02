// === FILENAME: src/host/capture.rs ===
use crate::host::config::CaptureConfig;
use crate::host::ffmpeg::FfmpegEncoder;
use crate::host::HostEvent;
use crate::logging::append_log;
use crate::protocol::HostMessage;
use image::Rgba;
use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use xcap::Monitor;

pub fn start_stream(stream: TcpStream, cfg: CaptureConfig, tx: UnboundedSender<HostEvent>) -> JoinHandle<()> {
    println!("[Capture] Inicjalizacja przechwytywania (H.264/FFmpeg)...");

    // Używamy spawn_blocking, ponieważ operacje na obrazie i enkoderze mogą blokować.
    tokio::task::spawn_blocking(move || {
        let stream = match stream.into_std() {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("[Capture] Błąd konwersji strumienia: {}", e);
                append_log(&msg);
                println!("{}", msg);
                return;
            }
        };
        let _ = stream.set_nodelay(true);

        let encoder = match FfmpegEncoder::new(cfg.width, cfg.height, cfg.fps.load(Ordering::Relaxed)) {
            Ok(enc) => enc,
            Err(e) => {
                let msg = format!("[Capture] Nie udało się uruchomić enkodera FFmpeg: {}", e);
                append_log(&msg);
                println!("{}", msg);
                return;
            }
        };

        let (mut writer, mut reader) = encoder.split();

        // Wątek do czytania z stdout FFmpeg i wysyłania do klienta
        let mut reader_stream = stream.try_clone().expect("Nie udało się sklonować strumienia");
        let reader_tx = tx.clone();
        let reader_handle = std::thread::spawn(move || {
            while let Some(chunk) = reader.read_encoded_chunk() {
                let wiadomosc = HostMessage::KlatkaObrazu { dane: chunk };
                if let Ok(payload) = bincode::serialize(&wiadomosc) {
                    let len = payload.len() as u32;
                    if reader_stream.write_all(&len.to_be_bytes()).is_err() || reader_stream.write_all(&payload).is_err() {
                    let msg = "[Capture] Klient (wątek czytający) się rozłączył.";
                    append_log(msg);
                    println!("{}", msg);
                    let _ = reader_tx.send(HostEvent::ClientDisconnected);
                    break;
                    }
                }
            }
            let msg = "[Capture] Wątek czytający FFmpeg zakończył działanie.";
            append_log(msg);
            println!("{}", msg);
        });


        let monitors = match Monitor::all() {
            Ok(monitors) => {
                if monitors.is_empty() {
                    let msg = "[Capture] Krytyczny błąd: Nie znaleziono żadnych monitorów.";
                    append_log(msg);
                    println!("{}", msg);
                    return;
                }
                monitors
            }
            Err(e) => {
                let msg = format!("[Capture] Nie udało się pobrać listy monitorów: {}", e);
                append_log(&msg);
                println!("{}", msg);
                return;
            }
        };

        let monitor = monitors.into_iter().find(|m| m.is_primary()).unwrap_or_else(|| {
            let msg = "[Capture] Nie znaleziono głównego monitora, wybieram pierwszy z listy.";
            append_log(msg);
            println!("{}", msg);
            Monitor::all().unwrap().remove(0)
        });

        loop {
            let loop_start = std::time::Instant::now();
            let current_fps = cfg.fps.load(Ordering::Relaxed).max(1);
            let frame_delay = Duration::from_millis(1000 / current_fps as u64);

            let frame = match monitor.capture_image() {
                Ok(frame) => {
                    append_log("Przechwycono klatkę.");
                    frame
                }
                Err(error) => {
                    append_log(&format!("[Capture] Błąd przechwytywania: {}", error));
                    println!("[Capture] Błąd przechwytywania: {}", error);
                    break;
                }
            };

            // xcap zwraca już gotowy `image::RgbaImage`
            let final_image = if (frame.width() != cfg.width) || (frame.height() != cfg.height) {
                image::imageops::resize(&frame, cfg.width, cfg.height, image::imageops::FilterType::Nearest)
            } else {
                frame
            };

            // Konwersja RGBA do RGB (ffmpeg oczekuje -pix_fmt rgb24)
            let mut rgb_data = Vec::with_capacity((cfg.width * cfg.height * 3) as usize);
            for Rgba([r, g, b, _]) in final_image.pixels() {
                rgb_data.push(*r);
                rgb_data.push(*g);
                rgb_data.push(*b);
            }

            if writer.write_frame(&rgb_data).is_err() {
                let msg = "[Capture] Błąd zapisu klatki do FFmpeg (prawdopodobnie proces zakończony).";
                append_log(msg);
                println!("{}", msg);
                break;
            }

            let elapsed = loop_start.elapsed();
            if elapsed < frame_delay {
                std::thread::sleep(frame_delay - elapsed);
            }
        }
        
        // Czekaj na zakończenie wątku czytającego, aby upewnić się, że wszystkie dane zostały wysłane
        let _ = reader_handle.join();
    })
}