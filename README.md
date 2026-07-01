# 2ndScreen

**2ndScreen** to lekka, szybka i asynchroniczna aplikacja do współdzielenia ekranu w sieci lokalnej, napisana w całości w języku **Rust**. Projekt został stworzony z myślą o wydajności i minimalnych opóźnieniach przy przesyłaniu obrazu z komputera Host na urządzenie Klienta.

## 🚀 Kluczowe cechy

* **Wysoka wydajność:** Wykorzystanie biblioteki `Tokio` do obsługi sieci oraz `xcap` do błyskawicznego przechwytywania klatek ekranu.
* **Automatyczne wykrywanie:** Dzięki protokołowi `mDNS` (Bonjour/Avahi), Twoje urządzenia same znajdują się w sieci.
* **Bezpieczeństwo:** Wymagana autoryzacja hasłem przy nawiązywaniu połączenia.
* **Multi-platform:** Napisany w Rust, co zapewnia stabilność i szybkość na systemach Linux, Windows oraz macOS.

## 🛠 Stos technologiczny

* **Język:** Rust
* **Sieć:** `Tokio` (Async TCP), `mdns-sd`
* **Grafika:** `xcap` (zrzuty ekranu), `minifb` (renderowanie okna), `image` (kompresja JPEG)
* **Serializacja:** `serde` + `bincode`

## 📦 Instalacja i uruchomienie

### Wymagania

* Zainstalowany [Rust](https://rustup.rs/) (cargo).
* Na systemach Linux (Arch/CachyOS): `libxcb`, `libx11` (pakiety deweloperskie).

### Budowanie

1. Sklonuj repozytorium:
```bash
git clone https://github.com/krowjarz/2ndscreen.git
cd second_screen

```


2. Skompiluj projekt:
```bash
cargo build --release

```



### Uruchomienie

`cargo run`


## 🗺 Plan rozwoju

* [ ] Implementacja sterowania myszką i klawiaturą (`enigo`).
* [ ] Przejście na kodek wideo H.264/FFmpeg dla lepszej kompresji.
* [ ] Budowa GUI w `egui` dla wygodniejszego zarządzania.
* [ ] Obsługa automatycznego wznawiania połączeń po zerwaniu sieci.

---

*Projekt tworzony z pasją do systemów niskopoziomowych i Rusta.*

## vibecoding go brr
