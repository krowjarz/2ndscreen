#[derive(Clone, Copy, Debug)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        CaptureConfig { width: 1280, height: 720, fps: 30 }
    }
}
