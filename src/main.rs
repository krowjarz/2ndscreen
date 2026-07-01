mod client;
mod host;
mod protocol;
mod gui;

/// Bez #[tokio::main] — eframe blokuje wątek główny własną pętlą zdarzeń
/// okna, więc runtime Tokio uruchamiamy ręcznie w tle i przekazujemy do
/// GUI tylko `Handle`, żeby móc spawnować zadania sieciowe z callbacków UI.
fn main() -> eframe::Result<()> {
    let runtime = tokio::runtime::Runtime::new().expect("Nie udało się utworzyć runtime Tokio");
    let handle = runtime.handle().clone();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([960.0, 680.0]),
        ..Default::default()
    };

    // `runtime` żyje aż do zakończenia run_native (czyli do zamknięcia
    // okna), więc jego wątki robocze działają przez cały czas życia apki.
    eframe::run_native(
        "2ndScreen",
        options,
        Box::new(move |_cc| Ok(Box::new(gui::App::new(handle)))),
    )
}
