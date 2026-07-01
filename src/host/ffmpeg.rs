use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use image::imageops::FilterType;
use xcap::Monitor;

use crate::host::config::CaptureConfig;
use crate::protocol::HostMessage;

fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(rgba.len());
    for chunk in rgba.chunks_exact(4) {
        let [r, g, b, a] = [chunk[0], chunk[1], chunk[2], chunk[3]];
        bgra.extend_from_slice(&[b, g, r, a]);
    }
    bgra
}

fn write_message(stream: &mut TcpStream, message: &HostMessage) {
    if let Ok(payload) = bincode::serialize(message) {
        let len = payload.len() as u32;
        let _ = stream.write_all(&len.to_be_bytes());
        let _ = stream.write_all(&payload);
    }
}

pub fn start_stream(mut stream: TcpStream, cfg: CaptureConfig) {
    println!("[FFmpeg] Rozpoczynam stream wideo przez FFmpeg...");

    let width = cfg.width;
    let height = cfg.height;
    let fps = cfg.fps.max(1);

    thread::spawn(move || {
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

        let monitory = Monitor::all().expect("Nie udało się pobrać listy monitorów");
        if monitory.is_empty() {
            println!("[FFmpeg] Brak monitorów, nie mogę rozpocząć streamu.");
            return;
        }

        let glowny_monitor = &monitory[0];
        let frame_delay = if fps > 0 { 1000u64 / fps as u64 } else { 33 };

        loop {
            if let Ok(klatka) = glowny_monitor.capture_image() {
                let final_image = if (klatka.width() != width) || (klatka.height() != height) {
                    image::imageops::resize(&klatka, width, height, FilterType::Lanczos3)
                } else {
                    klatka
                };

                let rgba = final_image.into_raw();
                let bgra = rgba_to_bgra(&rgba);
                if stdin.write_all(&bgra).is_err() || stdin.flush().is_err() {
                    break;
                }

                let mut chunk = [0u8; 8192];
                let n = stdout.read(&mut chunk).unwrap_or(0);
                if n > 0 {
                    let frame = HostMessage::VideoFrame { dane: chunk[..n].to_vec() };
                    write_message(&mut stream, &frame);
                }
            }

            thread::sleep(Duration::from_millis(frame_delay));
        }
    });
}
