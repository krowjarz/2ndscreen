use tokio::net::TcpStream;

use crate::host::config::CaptureConfig;

pub fn start_stream(stream: TcpStream, cfg: CaptureConfig) {
    let stream = stream.into_std().expect("Nie udało się zamienić TcpStream na std::net::TcpStream");
    crate::host::ffmpeg::start_stream(stream, cfg);
}
