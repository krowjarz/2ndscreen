// === FILENAME: src/host/capture.rs ===
use tokio::net::TcpStream;
use scrap::{Capturer, Display};
use std::io::{ErrorKind, Write};
use std::time::Duration;
use image::codecs::jpeg::JpegEncoder;
use image::RgbImage;
use crate::protocol::HostMessage;
use crate::host::config::CaptureConfig;
use std::sync::atomic::Ordering;
use crate::logging;

fn write_message(stream: &mut std::net::TcpStream, message: &HostMessage) {
    if let Ok(payload) = bincode::serialize(message) {
        let len = payload.len() as u32;
        let _ = stream.write_all(&len.to_be_bytes());
        let _ = stream.write_all(&payload);
    }
}

pub fn start_stream(stream: TcpStream, cfg: CaptureConfig) {
    println!("[Capture] Inicjalizacja przechwytywania ekranu (scrap)...");
    let mut stream = stream.into_std().expect("Nie udało się zamienić TcpStream na std::net::TcpStream");

    if let Err(e) = stream.set_nodelay(true) {
        let msg = format!("[Capture] Ostrzeżenie: nie udało się ustawić TCP_NODELAY: {}", e);
        println!("{}", msg);
        logging::append_log(&msg);
    } else {
        let msg = "[Capture] TCP_NODELAY ustawione na true".to_string();
        println!("{}", msg);
        logging::append_log(&msg);
    }

    tokio::task::spawn_blocking(move || {
        let display = Display::primary().expect("Nie udało się pobrać głównego ekranu");
        let mut capturer = Capturer::new(display).expect("Nie udało się utworzyć capturera ekranu");
        let monitor_width = capturer.width();
        let monitor_height = capturer.height();
        println!("[Capture] Rozpoczynam stream z monitora: {}x{}", monitor_width, monitor_height);

        let mut jpeg_data = Vec::new();
        let mut frame_index = 0usize;

        loop {
            let loop_start = std::time::Instant::now();
            let fps = cfg.fps.load(Ordering::Relaxed).max(1);
            let frame_delay = 1000u64 / fps as u64;

            let mut encoded = 0usize;
            let convert_ms;
            let mut resize_ms = 0u128;
            let mut encode_ms = 0u128;
            let mut write_ms = 0u128;

            let frame = loop {
                match capturer.frame() {
                    Ok(frame) => break frame,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => {
                        let msg = format!("[Capture] Błąd przechwytywania ekranu: {}", error);
                        println!("{}", msg);
                        logging::append_log(&msg);
                        return;
                    }
                }
            };

            let capture_ms = loop_start.elapsed().as_millis();
            let convert_start = std::time::Instant::now();

            // Szybka konwersja BGRA (Scrap) do RGB (Image)
            let mut rgb_data = Vec::with_capacity(monitor_width * monitor_height * 3);
            for chunk in frame.chunks_exact(4) {
                // chunk: [B, G, R, A]
                rgb_data.push(chunk[2]); // R
                rgb_data.push(chunk[1]); // G
                rgb_data.push(chunk[0]); // B
            }
            
            let img_buffer = RgbImage::from_raw(monitor_width as u32, monitor_height as u32, rgb_data)
                .expect("Błąd podczas tworzenia bufora obrazu");

            convert_ms = convert_start.elapsed().as_millis();

            let final_image: RgbImage = if (monitor_width as u32 != cfg.width) || (monitor_height as u32 != cfg.height) {
                let resize_start = std::time::Instant::now();
                // UWAGA: Używamy FilterType::Nearest. Jest znacznie szybszy na CPU niż Triangle!
                let resized = image::imageops::resize(&img_buffer, cfg.width, cfg.height, image::imageops::FilterType::Nearest);
                resize_ms = resize_start.elapsed().as_millis();
                resized
            } else {
                img_buffer
            };

            let enc_start = std::time::Instant::now();
            jpeg_data.clear();
            let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_data, 50); // Lekko podniosłem jakość, skoro oszczędzamy na CPU

            if encoder.encode_image(&final_image).is_ok() {
                encode_ms = enc_start.elapsed().as_millis();
                encoded = jpeg_data.len();

                let write_start = std::time::Instant::now();
                let jpeg_payload = std::mem::take(&mut jpeg_data);
                let wiadomosc = HostMessage::VideoFrame { dane: jpeg_payload };
                write_message(&mut stream, &wiadomosc);
                write_ms = write_start.elapsed().as_millis();
            }

            let loop_ms = loop_start.elapsed().as_millis();
            let info = format!("[Capture] loop_ms={} capture_ms={} convert_ms={} resize_ms={} encode_ms={} write_ms={} fps={} sent_bytes={}", 
                loop_ms, capture_ms, convert_ms, resize_ms, encode_ms, write_ms, fps, encoded);

            if frame_index % 10 == 0 {
                logging::append_log(&info);
            }
            if frame_index % 30 == 0 {
                println!("{}", info);
            }
            
            frame_index = frame_index.wrapping_add(1);

            let time_spent = loop_start.elapsed().as_millis() as u64;
            if frame_delay > time_spent {
                std::thread::sleep(Duration::from_millis(frame_delay - time_spent));
            }
        }
    });
}