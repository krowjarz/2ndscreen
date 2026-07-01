use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use xcap::Monitor;
use std::time::Duration;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use crate::protocol::HostMessage;
use crate::host::config::CaptureConfig;

pub async fn start_stream(mut stream: TcpStream, cfg: CaptureConfig) {
    println!("[Capture] Inicjalizacja przechwytywania ekranu (xcap)...");

    // Pobieramy listę wszystkich podłączonych monitorów
    let monitory = Monitor::all().expect("Nie udało się pobrać listy monitorów");
    
    if monitory.is_empty() {
        println!("[Capture] BŁĄD: Brak podłączonych monitorów!");
        return;
    }

    // Wybieramy pierwszy monitor z brzegu
    let glowny_monitor = &monitory[0];
    println!("[Capture] Rozpoczynam stream z monitora: {}", glowny_monitor.name());

    let frame_delay = if cfg.fps > 0 { 1000u64 / cfg.fps as u64 } else { 33 };

    loop {
        // xcap robi zrzut i od razu oddaje nam go w formacie RgbaImage
        if let Ok(klatka) = glowny_monitor.capture_image() {
            // Jeśli żądana rozdzielczość różni się, skalujemy obraz
            let final_image = if (klatka.width() != cfg.width) || (klatka.height() != cfg.height) {
                image::imageops::resize(&klatka, cfg.width, cfg.height, FilterType::Lanczos3)
            } else {
                klatka
            };

            let mut jpeg_data = Vec::new();
            
            // Kompresujemy klatkę do JPEG
            let mut encoder = JpegEncoder::new(&mut jpeg_data);
            if encoder.encode_image(&final_image).is_ok() {
                
                // Pakujemy obraz w nasz protokół sieciowy
                let wiadomosc = HostMessage::KlatkaObrazu { dane: jpeg_data };
                
                if let Ok(zserializowana) = bincode::serialize(&wiadomosc) {
                    
                    // Wysyłamy najpierw rozmiar paczki, żeby Klient wiedział ile bajtów odebrać
                    let len = zserializowana.len() as u32;
                    if stream.write_all(&len.to_be_bytes()).await.is_err() {
                        println!("[Capture] Klient rozłączył się. Zatrzymuję stream.");
                        break;
                    }
                    
                    // Wysyłamy właściwy, skompresowany obraz
                    if stream.write_all(&zserializowana).await.is_err() {
                        println!("[Capture] Błąd wysyłania klatki. Zatrzymuję stream.");
                        break;
                    }
                }
            }
        }
        
        // Czekamy wg ustawionego FPS
        tokio::time::sleep(Duration::from_millis(frame_delay)).await;
    }
}