use serde::{Deserialize, Serialize};

// Definiujemy wiadomości, jakie Klient może wysłać do Hosta
#[derive(Serialize, Deserialize, Debug)]
pub enum ClientMessage {
    Autoryzacja { haslo: String },
    ZmienKonfiguracje { rozdzielczosc: (u32, u32), fps: u32 },
    RuchMyszki { x: i32, y: i32 },
    KlikniecieMyszki { przycisk: String, wcisniety: bool },
    Klawisz { kod: u32, wcisniety: bool },
}

// Definiujemy wiadomości, jakie Host wysyła do Klienta
#[derive(Serialize, Deserialize, Debug)]
pub enum HostMessage {
    AutoryzacjaOk,
    AutoryzacjaBlad,
    KlatkaObrazu { dane: Vec<u8> }, // Tutaj będą lecieć bajty skompresowanego JPG
}
