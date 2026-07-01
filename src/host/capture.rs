use tokio::net::TcpStream;
use xcap::Monitor;
<<<<<<< HEAD
use std::io::Write;
use std::time::Duration;
use image::codecs::jpeg::JpegEncoder;
=======
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;
>>>>>>> c2e43394b39d1c98f8b50794296520f26f6f5267
use image::imageops::FilterType;
use crate::protocol::HostMessage;
use crate::host::config::CaptureConfig;

fn write_message(stream: &mut std::net::TcpStream, message: &HostMessage) {
    if let Ok(payload) = bincode::serialize(message) {
        let len = payload.len() as u32;
        let _ = stream.write_all(&len.to_be_bytes());
        let _ = stream.write_all(&payload);
    }
}

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

<<<<<<< HEAD
        let frame_delay = if cfg.fps > 0 { 1000u64 / cfg.fps as u64 } else { 33 };

        loop {
            if let Ok(klatka) = glowny_monitor.capture_image() {
                let final_image = if (klatka.width() != cfg.width) || (klatka.height() != cfg.height) {
                    image::imageops::resize(&klatka, cfg.width, cfg.height, FilterType::Lanczos3)
=======
        let width = cfg.width;
        let height = cfg.height;
        let fps = cfg.fps.max(1);
        let frame_delay = if fps > 0 { 1000u64 / fps as u64 } else { 33 };

        let header = HostMessage::VideoHeader { width, height, fps };
        write_message(&mut stream, &header);

        let mut ffmpeg = Command::new("ffmpeg")
            .args([
                "-f", "rawvideo",
                "-pix_fmt", "bgra",
                "-s", &format!("{}x{}", width, height),
                "-r", &fps.to_string(),
                "-i", "-",
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-tune", "zerolatency",
                "-pix_fmt", "yuv420p",
                "-f", "h264",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Nie udało się uruchomić FFmpeg");

        let mut stdin = ffmpeg.stdin.take().expect("Brak stdin FFmpeg");
        let mut stdout = ffmpeg.stdout.take().expect("Brak stdout FFmpeg");

        loop {
            if let Ok(klatka) = glowny_monitor.capture_image() {
                let final_image = if (klatka.width() != width) || (klatka.height() != height) {
                    image::imageops::resize(&klatka, width, height, FilterType::Lanczos3)
>>>>>>> c2e43394b39d1c98f8b50794296520f26f6f5267
                } else {
                    klatka
                };

<<<<<<< HEAD
                let mut jpeg_data = Vec::new();
                let mut encoder = JpegEncoder::new(&mut jpeg_data);
                if encoder.encode_image(&final_image).is_ok() {
                    let wiadomosc = HostMessage::VideoFrame { dane: jpeg_data };
                    write_message(&mut stream, &wiadomosc);
=======
                let mut bgra = Vec::with_capacity((width * height * 4) as usize);
                for pixel in final_image.pixels() {
                    let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
                    bgra.extend_from_slice(&[b, g, r, a]);
                }

                if stdin.write_all(&bgra).is_err() || stdin.flush().is_err() {
                    println!("[Capture] FFmpeg nie przyjął wejścia. Zatrzymuję stream.");
                    break;
                }

                let mut chunk = [0u8; 8192];
                let n = stdout.read(&mut chunk).unwrap_or(0);
                if n > 0 {
                    let frame = HostMessage::VideoFrame { dane: chunk[..n].to_vec() };
                    write_message(&mut stream, &frame);
>>>>>>> c2e43394b39d1c98f8b50794296520f26f6f5267
                }
            }

            std::thread::sleep(Duration::from_millis(frame_delay));
        }
    });
}
