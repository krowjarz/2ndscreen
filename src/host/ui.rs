use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::time::Duration;
use crate::host::config::CaptureConfig;

pub fn show_ui() -> CaptureConfig {
    let mut width: u32 = 1280;
    let mut height: u32 = 720;
    let mut fps: u32 = 30;

    let win_w = 480_usize;
    let win_h = 200_usize;
    let mut buffer: Vec<u32> = vec![0; win_w * win_h];

    let mut window = Window::new(
        "2ndScreen - Ustawienia (Enter=OK, Esc=Anuluj)",
        win_w,
        win_h,
        WindowOptions::default(),
    ).expect("Nie udało się utworzyć okna UI");

    // Ogranicz częstotliwość aktualizacji (około 60 FPS)
    window.limit_update_rate(Some(Duration::from_millis(16)));

    while window.is_open() && !window.is_key_down(Key::Enter) && !window.is_key_down(Key::Escape) {
        // Regulacja rozdzielczości
        if window.is_key_pressed(Key::Up, KeyRepeat::No) {
            height = height.saturating_add(10);
        }
        if window.is_key_pressed(Key::Down, KeyRepeat::No) {
            height = height.saturating_sub(10);
        }
        if window.is_key_pressed(Key::Right, KeyRepeat::No) {
            width = width.saturating_add(10);
        }
        if window.is_key_pressed(Key::Left, KeyRepeat::No) {
            width = width.saturating_sub(10);
        }

        // Regulacja FPS
        if window.is_key_pressed(Key::Equal, KeyRepeat::No) {
            fps = fps.saturating_add(1);
        }
        if window.is_key_pressed(Key::Minus, KeyRepeat::No) {
            fps = fps.saturating_sub(1);
        }

        // Aktualizuj tytuł okna z aktualnymi ustawieniami
        let title = format!("2ndScreen - {}x{} @ {}fps (Enter=OK Esc=Anuluj)", width, height, fps);
        window.set_title(&title);

        // Prosty wizual: wypełnij tło gradientem zależnym od FPS
        for y in 0..win_h {
            for x in 0..win_w {
                let v = (((x + y) % 256) as u32) * (fps % 255);
                buffer[y * win_w + x] = 0xFF000000 | (v & 0x00FFFFFF);
            }
        }

        // Rysuj bufor
        let _ = window.update_with_buffer(&buffer, win_w, win_h);
    }

    if window.is_key_down(Key::Escape) {
        println!("UI anulowane przez użytkownika, używam ustawień domyślnych.");
        return CaptureConfig::default();
    }

    CaptureConfig { width, height, fps }
}
