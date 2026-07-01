use tokio::net::TcpStream;
use xcap::Monitor;
use std::io::Write;
use std::time::Duration;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use crate::protocol::HostMessage;
use crate::host::config::CaptureConfig;

pub fn start_stream(stream: TcpStream, cfg: CaptureConfig) {
    println!("[Capture] Inicjalizacja przechwytywania ekranu (xcap)...");

    let mut stream = stream.into_std().expect("Nie udało się zamienić TcpStream na std::net::TcpStream");

    tokio::task::spawn_blocking(move || {
        let monitory = Monitor::all().expect("Nie udało się pobrać listy monitorów");

        if monitory.is_empty() {
            println!("[Capture] BŁĄD: Brak podłączonych monitorów!");
            return;
        }

        let glowny_monitor = &monitory[0];
        println!("[Capture] Rozpoczynam stream z monitora: {}", glowny_monitor.name());

        let frame_delay = if cfg.fps > 0 { 1000u64 / cfg.fps as u64 } else { 33 };

        loop {
            if let Ok(klatka) = glowny_monitor.capture_image() {
                let final_image = if (klatka.width() != cfg.width) || (klatka.height() != cfg.height) {
                    image::imageops::resize(&klatka, cfg.width, cfg.height, FilterType::Lanczos3)
                } else {
                    klatka
                };

                let mut jpeg_data = Vec::new();
                let mut encoder = JpegEncoder::new(&mut jpeg_data);
                if encoder.encode_image(&final_image).is_ok() {
                    let wiadomosc = HostMessage::VideoFrame { dane: jpeg_data };
                    if let Ok(zserializowana) = bincode::serialize(&wiadomosc) {
                        let len = zserializowana.len() as u32;
                        if stream.write_all(&len.to_be_bytes()).is_err() {
                            println!("[Capture] Klient rozłączył się. Zatrzymuję stream.");
                            break;
                        }
                        if stream.write_all(&zserializowana).is_err() {
                            println!("[Capture] Błąd wysyłania klatki. Zatrzymuję stream.");
                            break;
                        }
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(frame_delay));
        }
    });
}
