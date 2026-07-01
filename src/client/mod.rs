pub mod input;

use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedSender;
use mdns_sd::ServiceDaemon;
use crate::protocol::{ClientMessage, HostMessage};

/// Zdarzenia wysyłane z zadań sieciowych klienta do wątku GUI.
#[derive(Debug, Clone)]
pub enum ClientEvent {
    Log(String),
    HostsFound(Vec<(String, String)>),
    Connected,
    AuthFailed(String),
    Frame { rgba: Vec<u8>, width: u32, height: u32 },
    Disconnected,
}

fn build_candidate_hosts() -> Vec<String> {
    let mut candidates = Vec::new();

    if let Ok(host) = std::env::var("SECOND_SCREEN_HOST") {
        if !host.trim().is_empty() {
            candidates.push(host.trim().to_string());
        }
    }
    if let Ok(hosts) = std::env::var("SECOND_SCREEN_HOSTS") {
        for host in hosts.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            candidates.push(host.to_string());
        }
    }

    candidates.push("127.0.0.1".to_string());
    candidates.push("localhost".to_string());

    if let Ok(ips) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in ips {
            let _ = name;
            if let std::net::IpAddr::V4(v4) = ip {
                if !v4.is_loopback() {
                    let octets = v4.octets();
                    let base = format!("{}.{}.{}", octets[0], octets[1], octets[2]);
                    for last in [1, 2, 3, 5, 10, 20, 50, 100, 101, 200, 254] {
                        candidates.push(format!("{}.{}", base, last));
                    }
                }
            }
        }
    }

    // Dodaj kilka typowych nazw hostów, jeśli są dostępne lokalnie.
    candidates.push("host".to_string());
    candidates.push("desktop".to_string());
    candidates.push("pc".to_string());

    candidates.sort();
    candidates.dedup();
    candidates
}

async fn probe_host(target: &str, ports: &[u16]) -> Option<String> {
    for port in ports {
        let addr = format!("{}:{}", target, port);
        match tokio::time::timeout(std::time::Duration::from_millis(250), TcpStream::connect(&addr)).await {
            Ok(Ok(mut stream)) => {
                let probe = bincode::serialize(&ClientMessage::Autoryzacja { haslo: String::new() }).ok()?;
                if stream.write_all(&probe).await.is_ok() {
                    let mut response = vec![0u8; 256];
                    if let Ok(n) = stream.read(&mut response).await {
                        if bincode::deserialize::<HostMessage>(&response[..n]).is_ok() {
                            return Some(addr);
                        }
                    }
                }
            }
            Ok(Err(_)) | Err(_) => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::build_candidate_hosts;

    #[test]
    fn build_candidate_hosts_contains_local_defaults() {
        let candidates = build_candidate_hosts();
        assert!(candidates.iter().any(|host| host == "127.0.0.1"));
        assert!(candidates.iter().any(|host| host == "localhost"));
    }
}

/// Odkrywa dostępne hosty w sieci lokalnej: najpierw skanowaniem
/// kandydackich adresów (patrz `build_candidate_hosts`/`probe_host`,
/// bez zmian względem Twojego ostatniego commitu), potem przez mDNS.
/// Wynik trafia kanałem `tx` do GUI zamiast do stdin/stdout.
pub async fn discover_hosts(local_only: bool, tx: &UnboundedSender<ClientEvent>) {
    let mut znalezione_hosty = Vec::new();
    let candidate_hosts = build_candidate_hosts();

    let _ = tx.send(ClientEvent::Log("Szukam hosta po adresach lokalnych i sieciowych...".into()));
    for host in &candidate_hosts {
        if let Some(addr) = probe_host(host, &[8080, 8081, 8082, 9090]).await {
            if !znalezione_hosty.iter().any(|(_, a): &(String, String)| a == &addr) {
                znalezione_hosty.push((host.clone(), addr));
            }
        }
    }

    if !local_only {
        let _ = tx.send(ClientEvent::Log("Próbuję również odnaleźć hosta przez mDNS...".into()));

        if let Ok(mdns) = ServiceDaemon::new() {
            if let Ok(receiver) = mdns.browse("_2ndscreen._tcp.local.") {
                let koniec_szukania = std::time::Instant::now() + std::time::Duration::from_secs(2);

                while std::time::Instant::now() < koniec_szukania {
                    if let Ok(event) = receiver.recv_timeout(std::time::Duration::from_millis(200)) {
                        if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
                            let nazwa = info.get_fullname().to_string();
                            if let Some(ip) = info.get_addresses().iter().next() {
                                let adres_pelny = format!("{}:{}", ip, info.get_port());
                                if !znalezione_hosty.iter().any(|(_, addr)| addr == &adres_pelny) {
                                    znalezione_hosty.push((nazwa, adres_pelny));
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        let _ = tx.send(ClientEvent::Log("Tryb lokalny aktywny: pomijam mDNS.".into()));
    }

    let _ = tx.send(ClientEvent::HostsFound(znalezione_hosty));
}

/// Łączy się z hostem pod `addr` i wysyła hasło. Zwraca gotowy do
/// odczytu strumień wideo albo opis błędu.
pub async fn connect_and_auth(addr: &str, password: &str) -> Result<TcpStream, String> {
    let mut stream = TcpStream::connect(addr).await.map_err(|e| format!("Błąd połączenia: {}", e))?;

    let wiadomosc = ClientMessage::Autoryzacja { haslo: password.to_string() };
    let zserializowana = bincode::serialize(&wiadomosc).map_err(|e| e.to_string())?;
    stream.write_all(&zserializowana).await.map_err(|e| format!("Błąd wysyłania hasła: {}", e))?;

    let mut odpowiedz_buf = vec![0u8; 256];
    let n = stream.read(&mut odpowiedz_buf).await.map_err(|e| format!("Błąd odczytu odpowiedzi: {}", e))?;

    match bincode::deserialize::<HostMessage>(&odpowiedz_buf[..n]) {
        Ok(HostMessage::AutoryzacjaOk) => Ok(stream),
        Ok(HostMessage::AutoryzacjaBlad) => Err("Odmowa dostępu! Złe hasło.".to_string()),
        _ => Err("Nieoczekiwana odpowiedź hosta.".to_string()),
    }
}

/// Odbiera klatki wideo i przekazuje je jako gotowe bufory RGBA do GUI
/// (zamiast rysować bezpośrednio w oknie minifb jak poprzednio — GUI
/// samo zamienia je na teksturę egui).
fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for chunk in bgra.chunks_exact(4) {
        let [b, g, r, a] = [chunk[0], chunk[1], chunk[2], chunk[3]];
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    rgba
}

pub async fn stream_video(mut stream: TcpStream, tx: UnboundedSender<ClientEvent>) {
    let _ = tx.send(ClientEvent::Connected);

    loop {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            let _ = tx.send(ClientEvent::Log("Host zakończył połączenie.".into()));
            break;
        }
        let paczka_rozmiar = u32::from_be_bytes(len_buf) as usize;

        let mut paczka = vec![0u8; paczka_rozmiar];
        if stream.read_exact(&mut paczka).await.is_err() {
            let _ = tx.send(ClientEvent::Log("Błąd odczytu klatki.".into()));
            break;
        }

        if let Ok(message) = bincode::deserialize::<HostMessage>(&paczka) {
            match message {
                HostMessage::VideoFrame { dane } | HostMessage::KlatkaObrazu { dane } => {
                    if let Ok(obraz) = image::load_from_memory(&dane) {
                        let rgba = obraz.to_rgba8();
                        let (width, height) = (rgba.width(), rgba.height());
                        let _ = tx.send(ClientEvent::Frame { rgba: rgba.into_raw(), width, height });
                    }
                }
                HostMessage::VideoHeader { .. } => {}
                _ => {}
            }
        }
    }

    let _ = tx.send(ClientEvent::Disconnected);
}