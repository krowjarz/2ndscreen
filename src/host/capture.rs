use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use xcap::Monitor;
use std::time::Duration;
use image::codecs::jpeg::JpegEncoder;
use crate::protocol::HostMessage;

pub async fn start_stream(mut stream: TcpStream) {
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

    loop {
        // xcap robi zrzut i od razu oddaje nam go w formacie RgbaImage
        if let Ok(klatka) = glowny_monitor.capture_image() {
            let mut jpeg_data = Vec::new();
            
            // Kompresujemy klatkę do JPEG
            let mut encoder = JpegEncoder::new(&mut jpeg_data);
            if encoder.encode_image(&klatka).is_ok() {
                
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
        
        // Czekamy ~33ms co daje około 30 klatek na sekundę
        tokio::time::sleep(Duration::from_millis(33)).await;
    }
}