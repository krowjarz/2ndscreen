pub mod input;

use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::io::{self, Write};
use std::net::IpAddr;
use mdns_sd::ServiceDaemon;
use minifb::{Window, WindowOptions, Key};
use image::EncodableLayout;
use crate::protocol::{ClientMessage, HostMessage};

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

pub async fn uruchom_klienta() {
    let local_only = std::env::var("SECOND_SCREEN_LOCAL_ONLY")
        .map(|v| v.eq_ignore_ascii_case("1") || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);

    let mut znalezione_hosty = Vec::new();
    let candidate_hosts = build_candidate_hosts();

    println!("[Klient] Szukam hosta po adresach lokalnych i sieciowych...");
    for host in &candidate_hosts {
        if let Some(addr) = probe_host(host, &[8080, 8081, 8082, 9090]).await {
            znalezione_hosty.push((host.clone(), addr.clone()));
            println!("[Klient] Znaleziono host pod {}", addr);
        }
    }

    if !local_only {
        println!("[mDNS] Próbuję również odnaleźć hosta przez mDNS...");

        let mdns = ServiceDaemon::new().expect("Nie można uruchomić mDNS");
        let receiver = mdns.browse("_2ndscreen._tcp.local.").expect("Błąd wyszukiwania");

        let koniec_szukania = std::time::Instant::now() + std::time::Duration::from_secs(2);
        
        while std::time::Instant::now() < koniec_szukania {
            if let Ok(event) = receiver.recv_timeout(std::time::Duration::from_millis(200)) {
                match event {
                    mdns_sd::ServiceEvent::ServiceResolved(info) => {
                        let nazwa = info.get_fullname().to_string();
                        if let Some(ip) = info.get_addresses().iter().next() {
                            let adres_pelny = format!("{}:{}", ip, info.get_port());
                            if !znalezione_hosty.iter().any(|(_, addr)| addr == &adres_pelny) {
                                znalezione_hosty.push((nazwa, adres_pelny));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    } else {
        println!("[Klient] Tryb lokalny aktywny: pomijam mDNS i sieć/VPN. Wpisz adres ręcznie.");
    }

    let wybrany_host_ip;
    let mut reczny_ip = String::new();

    // --- LOGIKA WYBORU ADRESU ---
    if znalezione_hosty.is_empty() {
        println!("Nie znaleziono hostów w sieci automatycznie.");
        print!("Wpisz adres IP/hostname hosta ręcznie (np. 127.0.0.1:8080 albo 100.x.x.x:8080): ");
        let _ = io::stdout().flush();
        io::stdin().read_line(&mut reczny_ip).unwrap();
        
        let ip = reczny_ip.trim().to_string();
        if ip.is_empty() {
            println!("Błąd: Nie podano adresu IP.");
            return;
        }
        wybrany_host_ip = ip;
    } else {
        println!("\nZnalezione hosty:");
        for (i, (nazwa, adres)) in znalezione_hosty.iter().enumerate() {
            println!("[{}] {} (Adres: {})", i + 1, nazwa, adres);
        }

        print!("Wybierz numer hosta: ");
        let _ = io::stdout().flush();
        let mut wybor = String::new();
        io::stdin().read_line(&mut wybor).unwrap();
        
        let indeks: usize = match wybor.trim().parse::<usize>() {
            Ok(num) if num > 0 && num <= znalezione_hosty.len() => num - 1,
            _ => {
                println!("Nieprawidłowy wybór.");
                return;
            }
        };
        wybrany_host_ip = znalezione_hosty[indeks].1.clone();
    }
    // ----------------------------

    print!("Podaj hasło: ");
    let _ = io::stdout().flush();
    let mut wpisane = String::new();
    io::stdin().read_line(&mut wpisane).unwrap();
    let podane_haslo = wpisane.trim().to_string();

    println!("Łączenie z {}...", wybrany_host_ip);

    // Przekazujemy wybrany_host_ip (teraz to zwykły String)
    if let Ok(mut stream) = TcpStream::connect(&wybrany_host_ip).await {
        println!("[Klient] Połączono. Wysyłam hasło...");
        let wiadomosc = ClientMessage::Autoryzacja { haslo: podane_haslo };

        if let Ok(zserializowana) = bincode::serialize(&wiadomosc) {
            let _ = stream.write_all(&zserializowana).await;
            
            let mut odpowiedz_buf = vec![0u8; 256];
            if let Ok(n) = stream.read(&mut odpowiedz_buf).await {
                if let Ok(odpowiedz) = bincode::deserialize::<HostMessage>(&odpowiedz_buf[..n]) {
                    match odpowiedz {
                        HostMessage::AutoryzacjaOk => {
                            println!("[Klient] Sukces! Rozpoczynam odbiór streamu...");
                            odbieraj_wideo(stream).await;
                        }
                        HostMessage::AutoryzacjaBlad => {
                            println!("[Klient] Odmowa dostępu! Złe hasło.");
                        }
                        _ => println!("[Klient] Nieoczekiwana odpowiedź."),
                    }
                }
            }
        }
    } else {
        println!("[Klient] Błąd połączenia z podanym adresem.");
    }
}

async fn odbieraj_wideo(mut stream: TcpStream) {
    let mut window = Window::new(
        "2ndScreen - Klient",
        1280,
        720,
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    ).expect("Nie udało się utworzyć okna");

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            println!("Host zakończył połączenie.");
            break;
        }
        let paczka_rozmiar = u32::from_be_bytes(len_buf) as usize;

        let mut paczka = vec![0u8; paczka_rozmiar];
        if stream.read_exact(&mut paczka).await.is_err() {
            break;
        }

        if let Ok(HostMessage::KlatkaObrazu { dane }) = bincode::deserialize(&paczka) {
            if let Ok(obraz) = image::load_from_memory(&dane) {
                let rgba = obraz.to_rgba8();
                let (width, height) = (rgba.width() as usize, rgba.height() as usize);
                
                let mut buffer: Vec<u32> = Vec::with_capacity(width * height);
                for pixel in rgba.pixels() {
                    let a = pixel[3] as u32;
                    let r = pixel[0] as u32;
                    let g = pixel[1] as u32;
                    let b = pixel[2] as u32;
                    buffer.push((a << 24) | (r << 16) | (g << 8) | b);
                }

                window.update_with_buffer(&buffer, width, height).unwrap();
            }
        }
    }
}