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
    let listener = if let Ok(l) = TcpListener::bind(&default_bind).await {
        l
    } else if let Ok(l) = TcpListener::bind("0.0.0.0:0").await {
        let _ = tx.send(HostEvent::Log(format!("Port {} zajęty, używam losowego.", default_bind.split(':').last().unwrap_or("8080"))));
        l
    } else if let Ok(l) = TcpListener::bind("127.0.0.1:0").await {
        let _ = tx.send(HostEvent::Log("Nie udało się nasłuchiwać na 0.0.0.0, używam tylko loopback.".into()));
        l
    } else {
        let _ = tx.send(HostEvent::Log("Krytyczny błąd: Nie udało się związać żadnego gniazda TCP.".into()));
        return;
    };
    let bind_display = match listener.local_addr() {
        Ok(addr) => addr.to_string(),
        Err(_) => "Nieznany adres".to_string(),
    };

    let local_port = listener.local_addr().map(|a| a.port()).unwrap_or(8080);
    let _ = tx.send(HostEvent::Log(format!("Serwer TCP nasłuchuje na {}", bind_display)));
    let _ = tx.send(HostEvent::ListenAddr { addr: bind_display.clone(), port: local_port });

    if let Ok(ips) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in ips {
            if let std::net::IpAddr::V4(v4) = ip {
                if !v4.is_loopback() {
                    let _ = tx.send(HostEvent::Log(format!("Dostępny adres: {}:{}", v4, local_port)));
                }
            }
        }
    }
    let _ = tx.send(HostEvent::Log(format!("Dla klienta możesz też użyć: {}:{}", hostname, local_port)));

    if let Some(mdns) = &mdns {
        match ServiceInfo::new(
            service_type,
            &instance_name,
            &format!("{}.local.", hostname),
            "",
            local_port,
            None,
        ) {
            Ok(service) => {
                if mdns.register(service).is_ok() {
                    let _ = tx.send(HostEvent::Log(format!(
                        "[mDNS] Rozgłaszam usługę jako: {}.local (port {})",
                        hostname, local_port
                    )));
                }
            }
            Err(e) => {
                let _ = tx.send(HostEvent::Log(format!("Błąd tworzenia usługi mDNS: {}", e)));
            }
        }
    }

    loop {
        let (mut stream, addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                let _ = tx.send(HostEvent::Log(format!("Błąd accept: {}", e)));
                break;
            }
        };
        let _ = tx.send(HostEvent::Log(format!("Klient połączył się z: {}", addr)));
        let password_clone = password.clone();
        let tx_clone = tx.clone();
        let cfg_clone = cfg.clone();

        tokio::spawn(async move {
            let mut buffer = vec![0u8; 1024];
            match stream.read(&mut buffer).await {
                Ok(n) if n > 0 => {
                    match bincode::deserialize::<ClientMessage>(&buffer[..n]) {
                        Ok(ClientMessage::Autoryzacja { haslo }) => {
                            if haslo == password_clone {
                                let _ = tx_clone.send(HostEvent::Log("Hasło poprawne!".into()));
                                if let Ok(odpowiedz) = bincode::serialize(&HostMessage::AutoryzacjaOk) {
                                    if stream.write_all(&odpowiedz).await.is_ok() {
                                        let _ = tx_clone.send(HostEvent::ClientConnected(addr.to_string()));
                                        let join_handle = capture::start_stream(stream, cfg_clone, tx_clone);
                                        let _ = join_handle.await;
                                        return;
                                    }
                                }
                            } else {
                                let _ = tx_clone.send(HostEvent::Log("BŁĄD: Złe hasło.".into()));
                                if let Ok(odpowiedz) = bincode::serialize(&HostMessage::AutoryzacjaBlad) {
                                    let _ = stream.write_all(&odpowiedz).await;
                                }
                            }
                        }
                        _ => {
                            let _ = tx_clone.send(HostEvent::Log("Nieprawidłowa wiadomość od klienta.".into()));
                        }
                    }
                }
                _ => {
                    let _ = tx_clone.send(HostEvent::Log("Klient rozłączył się przed autoryzacją.".into()));
                }
            }
            let _ = tx_clone.send(HostEvent::ClientDisconnected);
        });
    }
}
