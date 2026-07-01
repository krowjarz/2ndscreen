use tokio::net::TcpStream;
use scrap::{Capturer, Display};
use std::io::{ErrorKind, Write};
use std::time::Duration;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::ColorType;
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

        let mut rgb_pixels = vec![0u8; (monitor_width * monitor_height * 3) as usize];
        let mut resized_pixels = Vec::new();
        let mut jpeg_data = Vec::new();
        let mut frame_index = 0usize;

        loop {
            let loop_start = std::time::Instant::now();
            let fps = cfg.fps.load(Ordering::Relaxed).max(1);
            let frame_delay = 1000u64 / fps as u64;

            let mut encoded = 0usize;
            let capture_ms;
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

            capture_ms = loop_start.elapsed().as_millis();
            let convert_start = std::time::Instant::now();
            for (src, dst) in frame.chunks_exact(4).zip(rgb_pixels.chunks_exact_mut(3)) {
                dst[0] = src[2];
                dst[1] = src[1];
                dst[2] = src[0];
            }
            convert_ms = convert_start.elapsed().as_millis();

            let (encode_source, image_width, image_height) = if (monitor_width as u32 != cfg.width) || (monitor_height as u32 != cfg.height) {
                resized_pixels.resize((cfg.width * cfg.height * 3) as usize, 0);
                let resize_start = std::time::Instant::now();
                let src_width = monitor_width as u32;
                let src_height = monitor_height as u32;
                let dst_width = cfg.width;
                let dst_height = cfg.height;
                for dst_y in 0..dst_height {
                    let src_y = dst_y * src_height / dst_height;
                    let src_row = (src_y * src_width * 3) as usize;
                    let dst_row = (dst_y * dst_width * 3) as usize;
                    for dst_x in 0..dst_width {
                        let src_x = dst_x * src_width / dst_width;
                        let src_offset = src_row + (src_x * 3) as usize;
                        let dst_offset = dst_row + (dst_x * 3) as usize;
                        resized_pixels[dst_offset..dst_offset + 3].copy_from_slice(&rgb_pixels[src_offset..src_offset + 3]);
                    }
                }
                resize_ms = resize_start.elapsed().as_millis();
                (&resized_pixels, cfg.width, cfg.height)
            } else {
                (&rgb_pixels, monitor_width as u32, monitor_height as u32)
            };

            let enc_start = std::time::Instant::now();
            jpeg_data.clear();
            let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_data, 40);
            if encoder.encode(encode_source, image_width, image_height, ColorType::Rgb8.into()).is_ok() {
                encode_ms = enc_start.elapsed().as_millis();
                encoded = jpeg_data.len();
                let write_start = std::time::Instant::now();
                let jpeg_payload = std::mem::take(&mut jpeg_data);
                let wiadomosc = HostMessage::VideoFrame { dane: jpeg_payload };
                write_message(&mut stream, &wiadomosc);
                write_ms = write_start.elapsed().as_millis();
            }

            let loop_ms = loop_start.elapsed().as_millis();
            let info = format!("[Capture] loop_ms={} capture_ms={} convert_ms={} resize_ms={} encode_ms={} write_ms={} fps={} sent_bytes={}", loop_ms, capture_ms, convert_ms, resize_ms, encode_ms, write_ms, fps, encoded);
            if frame_index % 10 == 0 {
                logging::append_log(&info);
            }
            if frame_index % 30 == 0 {
                println!("{}", info);
            }
            frame_index = frame_index.wrapping_add(1);

            if frame_delay > loop_ms as u64 {
                std::thread::sleep(Duration::from_millis(frame_delay - loop_ms as u64));
            }
        }
    });
}
