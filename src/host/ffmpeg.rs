// === FILENAME: src/host/ffmpeg.rs ===
use std::process::{Command, Stdio, Child, ChildStdin, ChildStdout};
use std::io::{Write, Read};

pub struct FfmpegWriter {
    stdin: ChildStdin,
}

impl FfmpegWriter {
    pub fn write_frame(&mut self, rgb_data: &[u8]) -> Result<(), std::io::Error> {
        self.stdin.write_all(rgb_data)?;
        self.stdin.flush()
    }
}

pub struct FfmpegReader {
    stdout: ChildStdout,
}

impl FfmpegReader {
    pub fn read_encoded_chunk(&mut self) -> Option<Vec<u8>> {
        let mut buffer = vec![0u8; 4096]; // bufor na zakodowane pakiety H.264
        if let Ok(n) = self.stdout.read(&mut buffer) {
            if n > 0 {
                buffer.truncate(n);
                return Some(buffer);
            }
        }
        None
    }
}

pub struct FfmpegEncoder {
    process: Child,
}

impl FfmpegEncoder {
    pub fn new(width: u32, height: u32, fps: u32) -> Result<Self, std::io::Error> {
        // Uruchamiamy zainstalowany w systemie program ffmpeg
        let process = Command::new("ffmpeg")
            .args(&[
                "-f", "rawvideo",          // wejściowy format to surowe wideo
                "-pix_fmt", "rgb24",       // format pikseli RGB (3 bajty na piksel)
                "-s", &format!("{}x{}", width, height), // rozdzielczość ekranu
                "-r", &fps.to_string(),    // klatki na sekundę
                "-i", "-",                 // pobieraj dane ze stdin (nasz strumień w Rust)
                "-c:v", "libx264",         // koder H.264
                "-preset", "ultrafast",    // maksymalna szybkość, najmniejsze opóźnienie
                "-tune", "zerolatency",    // optymalizacja pod kątem streamingu live
                "-f", "h264",              // format wyjściowy to surowy strumień H.264
                "-"                        // wyrzucaj wynik na stdout (do przechwycenia w Rust)
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())         // ukrywamy logi ffmpeg, żeby nie zaśmiecać konsoli
            .spawn()?;

        Ok(FfmpegEncoder { process })
    }

    pub fn split(mut self) -> (FfmpegWriter, FfmpegReader) {
        (
            FfmpegWriter { stdin: self.process.stdin.take().unwrap() },
            FfmpegReader { stdout: self.process.stdout.take().unwrap() },
        )
    }
}