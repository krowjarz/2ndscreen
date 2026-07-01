pub mod capture;
pub mod virtual_display;
pub mod ui;
pub mod config;

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

    let local_only = std::env::var("SECOND_SCREEN_LOCAL_ONLY")
        .map(|v| v.eq_ignore_ascii_case("1") || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(true);

    let mdns = if local_only {
        println!("[Host] Tryb lokalny aktywny: pomijam mDNS i sieć/VPN.");
        None
    } else {
        Some(ServiceDaemon::new().expect("Nie udało się odpalić mDNS"))
    };
    let service_type = "_2ndscreen._tcp.local.";
    let instance_name = format!("{}_screen", hostname);

    println!("Host działa. Aktywuję wirtualny ekran...");
    virtual_display::stworz_ekran();

    // Domyślnie nie otwieramy okna UI na hoscie — informacje o adresie/porcie są wypisywane w terminalu.
    let config = if std::env::var_os("SECOND_SCREEN_UI").is_some() {
        println!("[Host] Uruchamiam okno ustawień (ustaw SECOND_SCREEN_UI=0, aby wyłączyć).");
        tokio::task::spawn_blocking(|| ui::show_ui())
            .await
            .expect("UI task panicked")
    } else {
        println!("[Host] Okno ustawień pominięte. Używam domyślnych ustawień: 1280x720 @ 30fps.");
        config::CaptureConfig::default()
    };

    println!("[Host] Wybrane ustawienia: {}x{} @ {}fps", config.width, config.height, config.fps);

    // W trybie lokalnym nasłuchujemy wyłącznie na loopback, żeby nie używać sieci/VPN.
    let default_bind = if local_only { "127.0.0.1:0".to_string() } else { "0.0.0.0:8080".to_string() };
    let mut bind_display = default_bind.clone();
    let listener = match TcpListener::bind(&bind_display).await {
        Ok(l) => {
            println!("[Host] Serwer TCP nasłuchuje na {}", bind_display);
            l
        }
        Err(e) => {
            eprintln!("[Host] Nie udało się związać {}: {}", bind_display, e);
            let fallback = "127.0.0.1:8080";
            match TcpListener::bind(fallback).await {
                Ok(l2) => {
                    bind_display = fallback.to_string();
                    println!("[Host] Fallback: nasłuchiwanie na {}", bind_display);
                    l2
                }
                Err(e2) => {
                    let ephemeral = "127.0.0.1:0";
                    match TcpListener::bind(ephemeral).await {
                        Ok(l3) => {
                            let local = l3.local_addr().expect("Brak adresu lokalnego");
                            bind_display = format!("{}", local);
                            println!("[Host] Nasłuchiwanie na przydzielonym porcie: {}", bind_display);
                            l3
                        }
                        Err(e3) => {
                            panic!("Nie udało się związać gniazda: {}", e3);
                        }
                    }
                }
            }
        }
    };

    // Zarejestruj mDNS używając faktycznego portu, na którym nasłuchujemy
    let local_port = listener.local_addr().map(|a| a.port()).unwrap_or(8080);

    // Spróbuj wykryć główny adres IP interfejsu wyjściowego (UDP do 8.8.8.8)
    let primary_ip = (|| {
        match std::net::UdpSocket::bind("0.0.0.0:0") {
            Ok(sock) => {
                if sock.connect(("8.8.8.8", 80)).is_ok() {
                    return sock.local_addr().ok().map(|a| a.ip());
                }
                None
            }
            Err(_) => None,
        }
    })();

    // Wyświetl informacje o IP/porcie dla użytkownika
    println!("[Host] Nasłuchuję na: {} (port {})", bind_display, local_port);
    println!("[Host] Dostępne adresy:");
    if let Some(ip) = primary_ip {
        println!("  - {}:{}", ip, local_port);
    }
    println!("  - 127.0.0.1:{}", local_port);
    println!("[Host] Dla klienta możesz też użyć: {}.local:{}", hostname, local_port);
    if let Some(mdns) = mdns {
        let my_service = ServiceInfo::new(
            service_type,
            &instance_name,
            &format!("{}.local.", hostname),
            "",
            local_port,
            None,
        ).expect("Błąd tworzenia usługi mDNS");
        mdns.register(my_service).expect("Nie udało się zarejestrować usługi mDNS");
        println!("[mDNS] Rozgłaszam usługę w sieci jako: {}.local (port {})", hostname, local_port);
    }

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
                        capture::start_stream(stream, config).await;
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