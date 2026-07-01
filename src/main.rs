mod client;
mod host;
mod protocol;

use std::io::{self, Write};

#[tokio::main]
async fn main() {
    println!("=================================");
    println!("    Witaj w aplikacji 2ndScreen  ");
    println!("=================================");
    println!("Wybierz tryb uruchomienia:");
    println!("[1] Uruchom jako HOST (Udostępnij ekran)");
    println!("[2] Uruchom jako KLIENT (Odbierz ekran)");
    print!("Twój wybór (1 lub 2): ");

    // Wymuszamy wyświetlenie tekstu zachęty przed wpisaniem danych
    let _ = io::stdout().flush();

    // Odczytujemy to, co użytkownik wpisze w terminalu
    let mut wybor = String::new();
    io::stdin()
        .read_line(&mut wybor)
        .expect("Nie udało się odczytać linii");

    // Oczyszczamy tekst z białych znaków i nowej linii (\n)
    let wybor = wybor.trim();

    println!("---------------------------------");

    match wybor {
        "1" => {
            println!("Wybór: HOST. Odpalam serwer...");
            host::uruchom_hosta().await;
        }
        "2" => {
            println!("Wybór: KLIENT. Łączę z hostem...");
            client::uruchom_klienta().await;
        }
        _ => {
            println!("Niepoprawny wybór! Uruchom program ponownie i wpisz 1 lub 2.");
        }
    }
}
