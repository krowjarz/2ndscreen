use tokio::net::TcpStream;
use scrap::{Capturer, Display};
use std::io::{ErrorKind, Write};
use std::time::Duration;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
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

        loop {
            let loop_start = std::time::Instant::now();
            let fps = cfg.fps.load(Ordering::Relaxed).max(1);
            let frame_delay = 1000u64 / fps as u64;

            let mut encoded = 0usize;
            let mut capture_ms = 0u128;
            let mut convert_ms = 0u128;
            let mut resize_ms = 0u128;
            let mut encode_ms = 0u128;
            let mut write_ms = 0u128;

            let frame = loop {
                match capturer.frame() {
                    Ok(frame) => break frame.to_vec(),
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

            capture_ms = loop_start.elapsed().as_millis();
            let mut rgba = Vec::with_capacity(frame.len());
            let convert_start = std::time::Instant::now();
            for chunk in frame.chunks_exact(4) {
                rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
            }
            convert_ms = convert_start.elapsed().as_millis();

            let image_buffer = image::ImageBuffer::from_raw(monitor_width as u32, monitor_height as u32, rgba)
                .expect("Nie udało się utworzyć bufora obrazu z przechwyconej ramki");
            let mut dynamic = image::DynamicImage::ImageRgba8(image_buffer);

            if (monitor_width as u32 != cfg.width) || (monitor_height as u32 != cfg.height) {
                let resize_start = std::time::Instant::now();
                let resized = image::imageops::resize(&dynamic, cfg.width, cfg.height, FilterType::Triangle);
                dynamic = image::DynamicImage::ImageRgba8(resized);
                resize_ms = resize_start.elapsed().as_millis();
            }

            let enc_start = std::time::Instant::now();
            let mut jpeg_data = Vec::new();
            let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_data, 40);
            if encoder.encode_image(&dynamic).is_ok() {
                encode_ms = enc_start.elapsed().as_millis();
                encoded = jpeg_data.len();
                let msg = format!("[Capture] Wysyłam JPEG o rozmiarze {}", jpeg_data.len());
                println!("{}", msg);
                logging::append_log(&msg);
                let write_start = std::time::Instant::now();
                let wiadomosc = HostMessage::VideoFrame { dane: jpeg_data };
                write_message(&mut stream, &wiadomosc);
                write_ms = write_start.elapsed().as_millis();
            }

            let loop_ms = loop_start.elapsed().as_millis();
            let info = format!("[Capture] loop_ms={} capture_ms={} convert_ms={} resize_ms={} encode_ms={} write_ms={} fps={} sent_bytes={}", loop_ms, capture_ms, convert_ms, resize_ms, encode_ms, write_ms, fps, encoded);
            println!("{}", info);
            logging::append_log(&info);

            if frame_delay > loop_ms as u64 {
                std::thread::sleep(Duration::from_millis(frame_delay - loop_ms as u64));
            }
        }
    });
}
