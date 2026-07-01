use std::sync::{Arc, atomic::AtomicU32};

#[derive(Clone, Debug)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub fps: Arc<AtomicU32>,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        CaptureConfig { width: 1280, height: 720, fps: Arc::new(AtomicU32::new(30)) }
    }
}
