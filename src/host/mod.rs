pub mod capture;
pub mod virtual_display;
pub mod config;
pub mod ffmpeg;

use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedSender;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use crate::protocol::{ClientMessage, HostMessage};
use crate::host::config::CaptureConfig;

/// Zdarzenia wysyłane z zadania sieciowego hosta do wątku GUI.
/// GUI odbiera je nieblokująco (`try_recv`) w pętli `update()`.
#[derive(Debug, Clone)]
pub enum HostEvent {
    Log(String),
    ListenAddr { addr: String, port: u16 },
    ClientConnected(String),
    ClientDisconnected,
}

/// Uruchamia serwer hosta: bind, mDNS, pętla akceptująca klientów.
/// Zamiast println!/stdin (jak w starej wersji konsolowej) wszystkie
/// informacje idą kanałem `tx` do GUI. Funkcja jest spawnowana jako
/// osobny task Tokio i może być przerwana z zewnątrz przez
/// `JoinHandle::abort()` (patrz `src/gui`).
pub async fn run_host(cfg: CaptureConfig, password: String, local_only: bool, tx: UnboundedSender<HostEvent>) {
    virtual_display::stworz_ekran();

    let hostname = hostname::get()
        .unwrap_or_else(|_| "NieznanyHost".into())
        .into_string()
        .unwrap_or_else(|_| "Host".to_string());
    let _ = tx.send(HostEvent::Log(format!("Twój hostname to: {}", hostname)));

    let mdns = if local_only {
        let _ = tx.send(HostEvent::Log("Tryb lokalny aktywny: nasłuchuję tylko na loopback.".into()));
        None
    } else {
        match ServiceDaemon::new() {
            Ok(daemon) => Some(daemon),
            Err(e) => {
                let _ = tx.send(HostEvent::Log(format!("Nie udało się odpalić mDNS: {}", e)));
                None
            }
        }
    };
    let service_type = "_2ndscreen._tcp.local.";
    let instance_name = format!("{}_screen", hostname);

    let default_bind = if local_only { "127.0.0.1:0".to_string() } else { "0.0.0.0:8080".to_string() };
    let mut bind_display = default_bind.clone();
    let listener = match TcpListener::bind(&bind_display).await {
        Ok(l) => l,
        Err(e) => {
            let _ = tx.send(HostEvent::Log(format!("Nie udało się związać {}: {}", bind_display, e)));
            let fallback_all = "0.0.0.0:0";
            match TcpListener::bind(fallback_all).await {
                Ok(l2) => {
                    bind_display = fallback_all.to_string();
                    l2
                }
                Err(_) => {
                    let fallback_loopback = "127.0.0.1:0";
                    match TcpListener::bind(fallback_loopback).await {
                        Ok(l3) => {
                            let local = l3.local_addr().expect("Brak adresu lokalnego");
                            bind_display = format!("{}", local);
                            l3
                        }
                        Err(e3) => {
                            let _ = tx.send(HostEvent::Log(format!("Nie udało się związać gniazda: {}", e3)));
                            return;
                        }
                    }
                }
            }
        }
    };

    // Zarejestruj mDNS używając faktycznego portu, na którym nasłuchujemy
    let local_port = listener.local_addr().map(|a| a.port()).unwrap_or(8080);
    let _ = tx.send(HostEvent::ListenAddr { addr: bind_display.clone(), port: local_port });

    let interface_ips = match local_ip_address::list_afinet_netifas() {
        Ok(ips) => ips.into_iter().filter_map(|(_, ip)| match ip {
            std::net::IpAddr::V4(v4) if !v4.is_loopback() => Some(v4.to_string()),
            _ => None,
        }).collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };

    println!("[Host] Nasłuchuję na: {} (port {})", bind_display, local_port);
    println!("[Host] Dostępne adresy:");
    for ip in &interface_ips {
        println!("  - {}:{}", ip, local_port);
    }
    println!("  - 127.0.0.1:{}", local_port);
    println!("[Host] Dla klienta możesz też użyć: {}:{}", hostname, local_port);
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
                    if haslo == password {
                        let _ = tx.send(HostEvent::Log("[Host] Hasło poprawne!".into()));
                        let odpowiedz = bincode::serialize(&HostMessage::AutoryzacjaOk).unwrap();
                        let _ = stream.write_all(&odpowiedz).await;
                        capture::start_stream(stream, cfg);
                    } else {
                        let _ = tx.send(HostEvent::Log("[Host] BŁĄD: Złe hasło.".into()));
                        let odpowiedz = bincode::serialize(&HostMessage::AutoryzacjaBlad).unwrap();
                        let _ = stream.write_all(&odpowiedz).await;
                    }
                }
            }
        }
    }
}