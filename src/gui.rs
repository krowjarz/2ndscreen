use eframe::egui;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::client::{self, ClientEvent};
use crate::host::{self, config::CaptureConfig, HostEvent};

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Menu,
    HostSetup,
    HostRunning,
    ClientSetup,
    ClientStreaming,
}

/// Główny stan aplikacji GUI. Wszystkie zadania sieciowe (host/klient)
/// są spawnowane na runtime Tokio (`handle`) i komunikują się z tym
/// stanem przez kanały mpsc, odczytywane nieblokująco w `update()`.
pub struct App {
    handle: Handle,
    screen: Screen,

    // --- Host ---
    host_password: String,
    host_width: u32,
    host_height: u32,
    host_fps: u32,
    host_local_only: bool,
    host_rx: Option<UnboundedReceiver<HostEvent>>,
    host_task: Option<JoinHandle<()>>,
    host_log: Vec<String>,
    host_listen_addr: Option<String>,
    host_client: Option<String>,

    // --- Klient ---
    client_local_only: bool,
    client_rx: Option<UnboundedReceiver<ClientEvent>>,
    client_tx: Option<UnboundedSender<ClientEvent>>,
    client_task: Option<JoinHandle<()>>,
    client_discovering: bool,
    client_hosts: Vec<(String, String)>,
    client_selected_addr: String,
    client_manual_addr: String,
    client_password: String,
    client_log: Vec<String>,
    client_error: Option<String>,
    client_texture: Option<egui::TextureHandle>,
    client_frame_size: (u32, u32),
}

impl App {
    pub fn new(handle: Handle) -> Self {
        Self {
            handle,
            screen: Screen::Menu,

            host_password: String::new(),
            host_width: 1280,
            host_height: 720,
            host_fps: 30,
            host_local_only: false,
            host_rx: None,
            host_task: None,
            host_log: Vec::new(),
            host_listen_addr: None,
            host_client: None,

            client_local_only: false,
            client_rx: None,
            client_tx: None,
            client_task: None,
            client_discovering: false,
            client_hosts: Vec::new(),
            client_selected_addr: String::new(),
            client_manual_addr: String::new(),
            client_password: String::new(),
            client_log: Vec::new(),
            client_error: None,
            client_texture: None,
            client_frame_size: (0, 0),
        }
    }

    // ---------- Odbiór zdarzeń z zadań Tokio ----------

    fn poll_host_events(&mut self) {
        if let Some(rx) = &mut self.host_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    HostEvent::Log(msg) => self.host_log.push(msg),
                    HostEvent::ListenAddr { addr, port } => {
                        self.host_listen_addr = Some(format!("{} (port {})", addr, port));
                    }
                    HostEvent::ClientConnected(addr) => self.host_client = Some(addr),
                    HostEvent::ClientDisconnected => self.host_client = None,
                }
            }
        }
    }

    fn poll_client_events(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &mut self.client_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    ClientEvent::Log(msg) => self.client_log.push(msg),
                    ClientEvent::HostsFound(hosts) => {
                        self.client_hosts = hosts;
                        self.client_discovering = false;
                    }
                    ClientEvent::Connected => {
                        self.client_error = None;
                        self.screen = Screen::ClientStreaming;
                    }
                    ClientEvent::AuthFailed(err) => {
                        self.client_error = Some(err);
                    }
                    ClientEvent::Frame { rgba, width, height } => {
                        let color_image = egui::ColorImage::from_rgba_unmultiplied([
                            width as usize,
                            height as usize,
                        ], &rgba);
                        let old_size = self.client_frame_size;
                        let new_size = (width, height);
                        // If size changed, recreate texture to avoid partial/cropped updates.
                        match &mut self.client_texture {
                            Some(tex) if old_size == new_size && old_size != (0, 0) => {
                                tex.set(color_image, egui::TextureOptions::LINEAR)
                            }
                            _ => {
                                self.client_texture = Some(ctx.load_texture(
                                    "video_frame",
                                    color_image,
                                    egui::TextureOptions::LINEAR,
                                ));
                            }
                        }
                        self.client_frame_size = new_size;
                    }
                    ClientEvent::Disconnected => {
                        self.client_log.push("Rozłączono.".to_string());
                        self.client_texture = None;
                        self.screen = Screen::ClientSetup;
                    }
                }
            }
        }
    }

    // ---------- Akcje ----------

    fn start_host(&mut self) {
        let (tx, rx) = mpsc::unbounded_channel();
        self.host_rx = Some(rx);
        self.host_log.clear();
        self.host_listen_addr = None;
        self.host_client = None;

        let cfg = CaptureConfig {
            width: self.host_width,
            height: self.host_height,
            fps: self.host_fps,
        };
        let password = self.host_password.clone();
        let local_only = self.host_local_only;

        let task = self.handle.spawn(async move {
            host::run_host(cfg, password, local_only, tx).await;
        });
        self.host_task = Some(task);
        self.screen = Screen::HostRunning;
    }

    fn stop_host(&mut self) {
        // Zadanie hosta pracuje w pętli bez wewnętrznego mechanizmu
        // anulowania (patrz src/host/mod.rs), więc zatrzymujemy je z
        // zewnątrz przez abort() — to bezpieczne i nie wymaga zmian
        // w pętli przechwytywania/wysyłania klatek.
        if let Some(task) = self.host_task.take() {
            task.abort();
        }
        self.host_rx = None;
        self.host_client = None;
        self.host_listen_addr = None;
        self.screen = Screen::Menu;
    }

    fn discover_hosts(&mut self) {
        let (tx, rx) = mpsc::unbounded_channel();
        self.client_rx = Some(rx);
        self.client_tx = Some(tx.clone());
        self.client_hosts.clear();
        self.client_discovering = true;
        self.client_log.clear();
        let local_only = self.client_local_only;

        self.handle.spawn(async move {
            client::discover_hosts(local_only, &tx).await;
        });
    }

    fn connect_to_host(&mut self) {
        self.client_error = None;
        let addr = self.client_manual_addr.trim().to_string();
        let password = self.client_password.clone();

        let tx = if let Some(tx) = &self.client_tx {
            tx.clone()
        } else {
            let (tx, rx) = mpsc::unbounded_channel();
            self.client_rx = Some(rx);
            self.client_tx = Some(tx.clone());
            tx
        };

        let task = self.handle.spawn(async move {
            match client::connect_and_auth(&addr, &password).await {
                Ok(stream) => client::stream_video(stream, tx).await,
                Err(err) => {
                    let _ = tx.send(ClientEvent::AuthFailed(err));
                }
            }
        });
        self.client_task = Some(task);
    }

    fn disconnect_client(&mut self) {
        if let Some(task) = self.client_task.take() {
            task.abort();
        }
        self.client_rx = None;
        self.client_tx = None;
        self.client_texture = None;
        self.client_hosts.clear();
        self.screen = Screen::ClientSetup;
    }

    // ---------- Ekrany ----------

    fn ui_menu(&mut self, ui: &mut egui::Ui) {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.heading("2ndScreen");
            ui.add_space(30.0);
            ui.label("Wybierz tryb uruchomienia:");
            ui.add_space(15.0);
            if ui
                .add_sized([260.0, 40.0], egui::Button::new("🖥  Uruchom jako HOST"))
                .clicked()
            {
                self.screen = Screen::HostSetup;
            }
            ui.add_space(8.0);
            if ui
                .add_sized([260.0, 40.0], egui::Button::new("💻  Uruchom jako KLIENT"))
                .clicked()
            {
                self.screen = Screen::ClientSetup;
            }
        });
    }

    fn ui_host_setup(&mut self, ui: &mut egui::Ui) {
        ui.heading("Ustawienia hosta");
        ui.add_space(10.0);

        egui::Grid::new("host_settings_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Hasło:");
                ui.add(egui::TextEdit::singleline(&mut self.host_password).password(true));
                ui.end_row();

                ui.label("Szerokość:");
                ui.add(egui::DragValue::new(&mut self.host_width).range(320..=3840));
                ui.end_row();

                ui.label("Wysokość:");
                ui.add(egui::DragValue::new(&mut self.host_height).range(240..=2160));
                ui.end_row();

                ui.label("FPS:");
                ui.add(egui::DragValue::new(&mut self.host_fps).range(1..=60));
                ui.end_row();

                ui.label("Tylko lokalnie (bez mDNS/sieci):");
                ui.checkbox(&mut self.host_local_only, "");
                ui.end_row();
            });

        ui.add_space(15.0);

        let start_enabled = !self.host_password.trim().is_empty();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(start_enabled, egui::Button::new("Uruchom serwer"))
                .clicked()
            {
                self.start_host();
            }
            if ui.button("Wróć").clicked() {
                self.screen = Screen::Menu;
            }
        });
        if !start_enabled {
            ui.colored_label(egui::Color32::YELLOW, "Ustaw hasło zabezpieczające przed uruchomieniem.");
        }
    }

    fn ui_host_running(&mut self, ui: &mut egui::Ui) {
        ui.heading("Host aktywny");
        ui.add_space(10.0);

        match &self.host_listen_addr {
            Some(addr) => {
                ui.label(format!("Nasłuchuję na: {}", addr));
            }
            None => {
                ui.label("Uruchamianie serwera...");
            }
        }

        match &self.host_client {
            Some(addr) => {
                ui.colored_label(egui::Color32::GREEN, format!("Połączony klient: {}", addr));
            }
            None => {
                ui.label("Czekam na klienta...");
            }
        }

        ui.add_space(10.0);
        if ui.button("Zatrzymaj serwer").clicked() {
            self.stop_host();
        }

        ui.add_space(10.0);
        ui.separator();
        ui.label("Log:");
        egui::ScrollArea::vertical()
            .max_height(280.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.host_log {
                    ui.label(line);
                }
            });
    }

    fn ui_client_setup(&mut self, ui: &mut egui::Ui) {
        ui.heading("Połącz z hostem");
        ui.add_space(10.0);

        ui.checkbox(&mut self.client_local_only, "Tylko lokalnie (pomiń mDNS)");

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.client_discovering, egui::Button::new("🔍 Szukaj hostów"))
                .clicked()
            {
                self.discover_hosts();
            }
            if self.client_discovering {
                ui.spinner();
                ui.label("Szukam...");
            }
        });

        ui.add_space(10.0);
        if !self.client_hosts.is_empty() {
            ui.label("Znalezione hosty:");
            egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                for (nazwa, addr) in self.client_hosts.clone() {
                    let selected = self.client_selected_addr == addr;
                    if ui
                        .selectable_label(selected, format!("{} — {}", nazwa, addr))
                        .clicked()
                    {
                        self.client_selected_addr = addr.clone();
                        self.client_manual_addr = addr;
                    }
                }
            });
            ui.add_space(10.0);
        }

        ui.label("Adres hosta (IP:port):");
        ui.text_edit_singleline(&mut self.client_manual_addr);

        ui.label("Hasło:");
        ui.add(egui::TextEdit::singleline(&mut self.client_password).password(true));

        ui.add_space(15.0);
        let can_connect = !self.client_manual_addr.trim().is_empty();
        ui.horizontal(|ui| {
            if ui.add_enabled(can_connect, egui::Button::new("Połącz")).clicked() {
                self.connect_to_host();
            }
            if ui.button("Wróć").clicked() {
                self.screen = Screen::Menu;
            }
        });

        if let Some(err) = &self.client_error {
            ui.colored_label(egui::Color32::RED, err);
        }

        if !self.client_log.is_empty() {
            ui.add_space(10.0);
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(120.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.client_log {
                        ui.label(line);
                    }
                });
        }
    }

    fn ui_client_streaming(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("2ndScreen — podgląd");
            if ui.button("Rozłącz").clicked() {
                self.disconnect_client();
            }
        });
        ui.add_space(5.0);

        if let Some(texture) = &self.client_texture {
            let (w, h) = self.client_frame_size;
            if w > 0 && h > 0 {
                let available = ui.available_size();
                let aspect = w as f32 / h as f32;
                let mut size = available;
                if size.x / size.y > aspect {
                    size.x = size.y * aspect;
                } else {
                    size.y = size.x / aspect;
                }
                ui.add(egui::Image::new((texture.id(), texture.size_vec2())).fit_to_exact_size(size));
            }
        } else {
            ui.label("Oczekiwanie na pierwszą klatkę...");
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_host_events();
        self.poll_client_events(ctx);

        // Podczas aktywnego streamu/hosta odświeżamy w kółko, żeby nowe
        // klatki/logi pojawiały się na bieżąco (dane napływają z zadań
        // Tokio, a nie z interakcji użytkownika, więc bez tego egui
        // czekałby na kolejny input zanim narysuje coś nowego).
        if self.screen == Screen::ClientStreaming || self.screen == Screen::HostRunning {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::CentralPanel::default().show(ctx, |ui| match self.screen {
            Screen::Menu => self.ui_menu(ui),
            Screen::HostSetup => self.ui_host_setup(ui),
            Screen::HostRunning => self.ui_host_running(ui),
            Screen::ClientSetup => self.ui_client_setup(ui),
            Screen::ClientStreaming => self.ui_client_streaming(ui),
        });
    }
}
