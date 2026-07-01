pub mod capture;
pub mod virtual_display;

use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::io::{self, Write};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use crate::protocol::{ClientMessage, HostMessage};

pub async fn uruchom_hosta() {
    print!("Ustaw hasło zabezpieczające dla tego ekranu: ");
    let _ = io::stdout().flush();
    let mut zabezpieczenie = String::new();
    io::stdin().read_line(&mut zabezpieczenie).unwrap();
    let poprawne_haslo = zabezpieczenie.trim().to_string();

    // Pobieramy nazwę komputera (Hostname) – jeśli się nie uda, damy domyślną
    let hostname = hostname::get().unwrap_or_else(|_| "NieznanyHost".into()).into_string().unwrap_or_else(|_| "Host".to_string());
    println!("Twój hostname to: {}", hostname);

    // --- START mDNS (Rozgłaszanie) ---
    let mdns = ServiceDaemon::new().expect("Nie udało się odpalić mDNS");
    let service_type = "_2ndscreen._tcp.local.";
    let instance_name = format!("{}_screen", hostname);
    
    // Rozgłaszamy port 8080 na naszym hostname
    let my_service = ServiceInfo::new(
        service_type,
        &instance_name,
        &format!("{}.local.", hostname),
        "", 
        8080,
        None,
    ).expect("Błąd tworzenia usługi mDNS");
    
    mdns.register(my_service).expect("Nie udało się zarejestrować usługi mDNS");
    println!("[mDNS] Rozgłaszam usługę w sieci jako: {}.local", hostname);
    // ---------------------------------

    println!("Host działa. Aktywuję wirtualny ekran...");
    virtual_display::stworz_ekran();

    let adres = "0.0.0.0:8080";
    let listener = TcpListener::bind(adres).await.unwrap();
    println!("[Host] Serwer TCP nasłuchuje na {}", adres);

    if let Ok((mut stream, addr)) = listener.accept().await {
        println!("[Host] Klient połączył się z: {}", addr);

        let mut buffer = vec![0u8; 1024];
        if let Ok(n) = stream.read(&mut buffer).await {
            if n > 0 {
                if let Ok(ClientMessage::Autoryzacja { haslo }) = bincode::deserialize::<ClientMessage>(&buffer[..n]) {
                    if haslo == poprawne_haslo {
                        println!("[Host] Hasło poprawne!");
                        let odpowiedz = bincode::serialize(&HostMessage::AutoryzacjaOk).unwrap();
                        let _ = stream.write_all(&odpowiedz).await;
                        capture::start_stream(stream).await;
                    } else {
                        println!("[Host] BŁĄD: Złe hasło.");
                        let odpowiedz = bincode::serialize(&HostMessage::AutoryzacjaBlad).unwrap();
                        let _ = stream.write_all(&odpowiedz).await;
                    }
                }
            }
        }
    }
}