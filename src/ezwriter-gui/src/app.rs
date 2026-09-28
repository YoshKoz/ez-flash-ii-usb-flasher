use eframe::egui;
use rfd::FileDialog;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::device;
use crate::theme::{self, IconButtonExt};

/// Decode Nintendo 156-byte logo bitmap into 24x52 pixel array
/// Format: column-major, 3 bytes per column (24 rows = 3*8), 52 columns
fn decode_nintendo_logo(data: &[u8]) -> Vec<[u8; 3]> {
    const BG: [u8; 3] = [0xE0, 0xE0, 0xE0];
    const FG: [u8; 3] = [0x40, 0x40, 0x40];
    let mut pixels = vec![BG; 24 * 52];
    if data.len() < 156 {
        return pixels;
    }
    for col in 0..52 {
        for row_group in 0..3 {
            let byte = data[col * 3 + row_group];
            if byte == 0 {
                continue;
            }
            for bit in 0..8 {
                let row = row_group * 8 + bit;
                if row >= 24 {
                    continue;
                }
                if (byte >> (7 - bit)) & 1 == 1 {
                    pixels[row * 52 + col] = FG;
                }
            }
        }
    }
    pixels
}

/// Parse a hex address string like `0x000000` or `000000`.
fn parse_hex_u32(s: &str) -> Option<u32> {
    let t = s.trim();
    let t = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    u32::from_str_radix(t, 16).ok()
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum AppTab {
    Status,
    CartInfo,
    ReadRom,
    WriteRom,
    ReadSave,
    WriteSave,
}

enum BgCmd {
    Status(String),
    Header(Box<Option<device::CartHeader>>),
    Progress(String),
    DumpProgress {
        bytes_read: u64,
        total_bytes: u64,
    },
    SaveReadProgress {
        bytes_read: u64,
        total_bytes: u64,
    },
    SaveWriteProgress {
        bytes_read: u64,
        total_bytes: u64,
    },
    RomWriteProgress {
        bytes_written: u64,
        total_bytes: u64,
    },
    Error(String),
    /// Box art for the detected cartridge; `None` when nothing matched.
    Banner(Box<Option<device::Banner>>),
}

pub struct EzWriterApp {
    tab: AppTab,
    status_text: String,
    cart_header: Option<device::CartHeader>,
    nintendo_logo: Vec<[u8; 3]>,
    /// Box art for the cartridge currently detected, once it has been fetched.
    banner: Option<egui::TextureHandle>,
    banner_status: String,
    /// Output list: every status, progress and error line, oldest first.
    log: Vec<String>,
    log_start: std::time::Instant,
    rom_path: PathBuf,
    save_path: PathBuf,
    /// Write ROM tab state.
    write_rom_path: PathBuf,
    write_rom_addr: String,
    write_rom_delay_ms: u64,
    write_rom_no_erase: bool,
    write_rom_verify: bool,
    write_rom_init: bool,
    /// Trim trailing 0xFF/0x00 padding before writing.
    write_rom_trim: bool,
    /// Optional IPS patch applied to the image before writing.
    ips_path: PathBuf,
    /// Save hardware the selected ROM was built for, from its SDK save-library
    /// marker. Shown on the Burn tab and used as a fallback by the save tabs
    /// when the cartridge's game code is not in the built-in database.
    rom_save_type: Option<&'static str>,
    /// Tracks the maximise state so the title bar can show Maximise or Restore.
    maximized: bool,
    /// Set when the selected file was a GB/GBC ROM wrapped with Goomba, holding
    /// a short description of the wrap. `None` for a plain GBA ROM.
    wrapped_from: Option<String>,
    progress: String,
    progress_value: f32,
    /// Read every ROM chunk twice and require agreement. Catches the stale-EP2
    /// and marginal-cartridge failures that otherwise corrupt a dump silently,
    /// at the cost of roughly doubling dump time.
    confirm_chunks: bool,
    tx: Sender<BgCmd>,
    rx: Receiver<BgCmd>,
}

impl Default for EzWriterApp {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tab: AppTab::Status,
            status_text: "Start: click Detect Device or Initialize".into(),
            cart_header: None,
            nintendo_logo: Vec::new(),
            banner: None,
            banner_status: String::new(),
            log: Vec::new(),
            log_start: std::time::Instant::now(),
            rom_path: PathBuf::new(),
            save_path: PathBuf::new(),
            write_rom_path: PathBuf::new(),
            rom_save_type: None,
            maximized: false,
            wrapped_from: None,
            write_rom_addr: "0x000000".into(),
            write_rom_delay_ms: 50,
            write_rom_no_erase: false,
            write_rom_verify: true,
            write_rom_init: true,
            write_rom_trim: false,
            ips_path: PathBuf::new(),
            progress: String::new(),
            progress_value: 0.0,
            confirm_chunks: true,
            tx,
            rx,
        }
    }
}

impl eframe::App for EzWriterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The root ui spans the viewport, so capture it before the panels below
        // start carving pieces off it. egui 0.36 has no Context::screen_rect.
        let screen = ui.max_rect();
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                BgCmd::Status(s) => {
                    self.log_push(&s);
                    self.status_text = s;
                }
                BgCmd::Header(h) => {
                    self.cart_header = *h;
                    let info = self
                        .cart_header
                        .as_ref()
                        .map(|hdr| (hdr.title.clone(), hdr.code.clone(), hdr.raw_header));
                    match info {
                        Some((title, code, raw)) => {
                            self.progress = format!("Cartridge: {title} [{code}]");
                            self.nintendo_logo = decode_nintendo_logo(&raw[4..160]);
                            // Box art is a network fetch, so do it off the UI thread.
                            self.banner = None;
                            self.banner_status = "looking for box art...".into();
                            let tx = self.tx.clone();
                            thread::spawn(move || {
                                let b = device::fetch_boxart(&title, &code).ok();
                                let _ = tx.send(BgCmd::Banner(Box::new(b)));
                            });
                        }
                        None => {
                            self.banner = None;
                            self.banner_status = String::new();
                        }
                    }
                }
                BgCmd::Banner(b) => match *b {
                    Some(banner) => {
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [banner.width, banner.height],
                            &banner.rgba,
                        );
                        self.banner = Some(ui.ctx().load_texture(
                            "boxart",
                            img,
                            egui::TextureOptions::LINEAR,
                        ));
                        self.banner_status = String::new();
                    }
                    None => {
                        self.banner = None;
                        self.banner_status = "no box art found for this cartridge".into();
                    }
                },
                BgCmd::Progress(s) => {
                    self.log_push(&s);
                    self.progress = s;
                }
                BgCmd::DumpProgress {
                    bytes_read,
                    total_bytes,
                } => {
                    let pct = bytes_read as f64 / total_bytes as f64;
                    self.progress_value = pct as f32;
                    self.progress = format!(
                        "Dumping ROM: {:.1} / {:.1} MB ({:.0}%)",
                        bytes_read as f64 / 1_048_576.0,
                        total_bytes as f64 / 1_048_576.0,
                        pct * 100.0
                    );
                }
                BgCmd::SaveReadProgress {
                    bytes_read,
                    total_bytes,
                } => {
                    let pct = bytes_read as f64 / total_bytes as f64;
                    self.progress_value = pct as f32;
                    self.progress = format!(
                        "Dumping save: {:.0}% ({:.1} / {:.1} KB)",
                        pct * 100.0,
                        bytes_read as f64 / 1024.0,
                        total_bytes as f64 / 1024.0
                    );
                }
                BgCmd::SaveWriteProgress {
                    bytes_read,
                    total_bytes,
                } => {
                    let pct = bytes_read as f64 / total_bytes as f64;
                    self.progress_value = pct as f32;
                    self.progress = format!(
                        "Writing save: {:.0}% ({:.1} / {:.1} KB)",
                        pct * 100.0,
                        bytes_read as f64 / 1024.0,
                        total_bytes as f64 / 1024.0
                    );
                }
                BgCmd::RomWriteProgress {
                    bytes_written,
                    total_bytes,
                } => {
                    let pct = bytes_written as f64 / total_bytes as f64;
                    self.progress_value = pct as f32;
                    self.progress = format!(
                        "Writing ROM: {:.0}% ({:.1} / {:.1} KB)",
                        pct * 100.0,
                        bytes_written as f64 / 1024.0,
                        total_bytes as f64 / 1024.0
                    );
                }
                BgCmd::Error(e) => {
                    self.log_push(&format!("Error: {e}"));
                    self.progress = format!("Error: {e}");
                    self.cart_header = None;
                    self.nintendo_logo.clear();
                    self.banner = None;
                    self.banner_status.clear();
                }
            }
        }

        self.show_title_bar(ui);

        egui::Panel::top("menu").show(ui, |ui| {
            self.show_toolbar(ui);
        });

        // Output list. The original's is an embedded browser pointed at the
        // vendor site, so it shows whatever that serves (a Cloudflare challenge,
        // the last time it was opened) rather than anything about the cartridge.
        // This one is the program's own log.
        let mut log = std::mem::take(&mut self.log);
        egui::Panel::bottom("output").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Output list");
                ui.separator();
                if ui.button("Clear").clicked() {
                    log.clear();
                }
                ui.label(format!("{} line(s)", log.len()));
            });
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &log {
                        ui.monospace(line);
                    }
                });
        });
        self.log = log;

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(200.0, ui.available_height()),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.add_space(2.0);
                        ui.strong("Sections");
                        ui.add_space(4.0);
                        for (t, icon, label) in [
                            (AppTab::Status, theme::icons::CHIP, "Device"),
                            (AppTab::CartInfo, theme::icons::CARTRIDGE, "Cartridge"),
                            (AppTab::ReadRom, theme::icons::DOWNLOAD, "Read ROM"),
                            (AppTab::WriteRom, theme::icons::FLASH, "Burn"),
                            (AppTab::ReadSave, theme::icons::SAVE, "Save backup"),
                            (AppTab::WriteSave, theme::icons::UPLOAD, "Save write"),
                        ] {
                            let text = egui::RichText::new(format!("  {icon}   {label}"));
                            if ui.selectable_label(self.tab == t, text).clicked() {
                                self.tab = t;
                            }
                        }
                        ui.separator();
                        ui.strong("ROM Lists");
                        ui.separator();
                        match &self.cart_header {
                            Some(hdr) => {
                                ui.label(format!("C  {}", hdr.title));
                                if !hdr.code.is_empty() {
                                    ui.label(format!("     [{}]", hdr.code));
                                }
                                ui.label(format!("     {}", hdr.save_type));
                            }
                            None => {
                                ui.label("C  (no cart)");
                            }
                        }
                        ui.separator();
                        ui.label("Tip: Refresh List after plugging in.");
                    },
                );
                ui.separator();
                // One scrolling page rather than tab-clipped panels. The
                // ScrollArea inherits its parent's layout, which here is
                // horizontal (we are inside `horizontal_top`), so the content
                // needs an explicit vertical wrapper — without it every widget
                // in a section lands on one very wide line and the ones past
                // the window edge (e.g. Eject) are unreachable.
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            theme::card(ui, |ui| match self.tab {
                                AppTab::Status => self.show_status(ui),
                                AppTab::CartInfo => self.show_cart_info(ui),
                                AppTab::ReadRom => self.show_read_rom(ui),
                                AppTab::WriteRom => self.show_write_rom(ui),
                                AppTab::ReadSave => self.show_read_save(ui),
                                AppTab::WriteSave => self.show_write_save(ui),
                            });
                        });
                    });
            });
        });

        // Last, so the thin edge bands win interaction over the panels.
        Self::resize_edges(ui, screen);
    }
}

impl EzWriterApp {
    /// Save hardware to act on.
    ///
    /// The cartridge's own type when its game code is in the built-in database,
    /// otherwise the save library detected from the selected ROM's SDK marker.
    /// That fallback is what lets the save tabs work on a game `GAME_DB` does
    /// not know — the case that previously refused with "UNKNOWN".
    fn effective_save_type(&self) -> String {
        if let Some(hdr) = &self.cart_header
            && device::is_known_save_type(&hdr.save_type)
        {
            return hdr.save_type.clone();
        }
        match self.rom_save_type {
            Some(t) => t.to_string(),
            None => self
                .cart_header
                .as_ref()
                .map_or_else(|| "UNKNOWN".to_string(), |h| h.save_type.clone()),
        }
    }

    /// Record a chosen ROM file and detect the save library it was built for.
    /// The markers sit megabytes in, so this is a full read; doing it here keeps
    /// the burn itself uninterrupted.
    /// Locate the Goomba loader shipped with the tooling.
    ///
    /// Checked beside the executable, in the working directory, and in
    /// `firmware`; the original client's `Sysbin\goomba.gba` is accepted last so
    /// an existing EZ Client install works as-is.
    fn find_goomba_loader() -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            candidates.push(dir.join("goomba.gba"));
            candidates.push(dir.join("firmware/goomba.gba"));
        }
        candidates.push(PathBuf::from("goomba.gba"));
        candidates.push(PathBuf::from("firmware/goomba.gba"));

        candidates.into_iter().find(|p| p.is_file())
    }

    /// Record a chosen ROM file and detect the save library it was built for.
    ///
    /// A Game Boy / Game Boy Color ROM cannot be flashed as-is: a GBA cannot
    /// execute it. It is wrapped with the Goomba emulator instead (see
    /// `device::wrap_gb_rom`) and the wrapped image becomes the write source.
    fn set_rom(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        self.wrapped_from = None;
        let source = if ext == "gb" || ext == "gbc" {
            self.wrap_game_boy_rom(&path, &ext).unwrap_or(path)
        } else {
            path
        };

        self.rom_save_type = std::fs::read(&source)
            .ok()
            .and_then(|data| device::detect_saver_from_rom(&data));
        self.write_rom_path = source;
    }

    /// Wrap a GB/GBC ROM with Goomba so it can be written to the cartridge.
    /// Returns the wrapped image path on success.
    fn wrap_game_boy_rom(&mut self, path: &std::path::Path, ext: &str) -> Option<PathBuf> {
        let Some(loader_path) = Self::find_goomba_loader() else {
            self.log_push(
                "Game Boy ROM selected but goomba.gba was not found. Put it beside the \
                 executable or in firmware/ — writing the raw ROM would not run.",
            );
            return None;
        };
        let (Ok(loader), Ok(gb)) = (std::fs::read(&loader_path), std::fs::read(path)) else {
            self.log_push("Could not read the ROM or the Goomba loader");
            return None;
        };
        let wrapped = match device::wrap_gb_rom(&loader, &gb) {
            Ok(w) => w,
            Err(e) => {
                self.log_push(&format!("Goomba wrap failed: {e}"));
                return None;
            }
        };
        // Write beside the source ROM rather than %TEMP%: the process can be
        // denied access outside its own tree (os error 5), and a path next to
        // the file the user picked is easier to find anyway.
        let out = path.with_extension("goomba.gba");
        if let Err(e) = std::fs::write(&out, &wrapped) {
            self.log_push(&format!("Could not write the wrapped image: {e}"));
            return None;
        }
        let title = device::gb_rom_title(&gb);
        self.wrapped_from = Some(format!(
            "{title} [{}] — Goomba {} + ROM {} bytes",
            ext.to_uppercase(),
            loader.len(),
            gb.len()
        ));
        self.log_push(&format!(
            "Wrapped {} ROM '{title}' with {} -> {} bytes",
            ext.to_uppercase(),
            loader_path.display(),
            wrapped.len()
        ));
        Some(out)
    }

    /// The app's own Fluent title bar. The OS frame is disabled, so this both
    /// draws the chrome and provides the behaviour a frame would have: dragging
    /// to move, double-click to maximise, and the three window controls.
    fn show_title_bar(&mut self, ui: &mut egui::Ui) {
        let bar_height = 32.0;
        let inner = egui::Panel::top("titlebar")
            .frame(egui::Frame::NONE.fill(theme::LAYER))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(bar_height);
                    ui.add_space(10.0);
                    let (icon_rect, _) =
                        ui.allocate_exact_size(egui::vec2(16.0, bar_height), egui::Sense::hover());
                    ui.painter().text(
                        icon_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        theme::icons::CHIP,
                        theme::icon_font(14.0),
                        theme::ACCENT,
                    );
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("EZ-Flash II USB Flasher").size(12.5));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::window_button(ui, theme::icons::CLOSE, "Close", true).clicked() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let glyph = if self.maximized {
                            theme::icons::RESTORE
                        } else {
                            theme::icons::MAXIMIZE
                        };
                        let name = if self.maximized {
                            "Restore"
                        } else {
                            "Maximise"
                        };
                        if theme::window_button(ui, glyph, name, false).clicked() {
                            self.maximized = !self.maximized;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(
                                self.maximized,
                            ));
                        }
                        if theme::window_button(ui, theme::icons::MINIMIZE, "Minimise", false)
                            .clicked()
                        {
                            ui.ctx()
                                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    });
                });
            });

        // Drag anywhere on the bar except over the window controls.
        let r = inner.response.rect;
        let drag_rect = egui::Rect::from_min_max(r.min, egui::pos2(r.right() - 150.0, r.bottom()));
        let resp = ui.interact(
            drag_rect,
            egui::Id::new("titlebar_drag"),
            egui::Sense::click_and_drag(),
        );
        if resp.dragged() {
            // ViewportCommand::StartDrag does not move this undecorated window on
            // Windows (BeginResize works, StartDrag does not), so move it by
            // applying the pointer delta to the outer position directly.
            let delta = ui.ctx().input(|i| i.pointer.delta());
            if delta != egui::Vec2::ZERO
                && let Some(outer) = ui.ctx().input(|i| i.viewport().outer_rect)
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::OuterPosition(outer.min + delta));
            }
        }
        if resp.double_clicked() {
            self.maximized = !self.maximized;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(self.maximized));
        }
    }

    /// Hit-test the window edges and hand them to the backend.
    ///
    /// With `with_decorations(false)` the OS draws no resize borders, so without
    /// this the window could be moved but never resized. Called last so the thin
    /// edge bands sit above the panel backgrounds.
    fn resize_edges(ui: &egui::Ui, s: egui::Rect) {
        use egui::viewport::ResizeDirection as Dir;
        let b = 6.0;
        let edges: [(egui::Rect, Dir); 8] = [
            (
                egui::Rect::from_min_max(s.min, egui::pos2(s.max.x, s.min.y + b)),
                Dir::North,
            ),
            (
                egui::Rect::from_min_max(egui::pos2(s.min.x, s.max.y - b), s.max),
                Dir::South,
            ),
            (
                egui::Rect::from_min_max(s.min, egui::pos2(s.min.x + b, s.max.y)),
                Dir::West,
            ),
            (
                egui::Rect::from_min_max(egui::pos2(s.max.x - b, s.min.y), s.max),
                Dir::East,
            ),
            (
                egui::Rect::from_min_max(s.min, s.min + egui::vec2(b, b)),
                Dir::NorthWest,
            ),
            (
                egui::Rect::from_min_max(
                    egui::pos2(s.max.x - b, s.min.y),
                    egui::pos2(s.max.x, s.min.y + b),
                ),
                Dir::NorthEast,
            ),
            (
                egui::Rect::from_min_max(
                    egui::pos2(s.min.x, s.max.y - b),
                    egui::pos2(s.min.x + b, s.max.y),
                ),
                Dir::SouthWest,
            ),
            (
                egui::Rect::from_min_max(s.max - egui::vec2(b, b), s.max),
                Dir::SouthEast,
            ),
        ];
        for (rect, dir) in edges {
            let resp = ui.interact(
                rect,
                egui::Id::new(("resize", dir as u8)),
                egui::Sense::drag(),
            );
            if resp.drag_started() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
            }
        }
    }

    /// Ask for a ROM file, then record it.
    fn pick_rom(&mut self) {
        if let Some(path) = FileDialog::new()
            .set_title("Open GBA ROM")
            .add_filter("GBA ROM", &["gba", "bin"])
            .add_filter("All Files", &["*"])
            .pick_file()
        {
            self.set_rom(path);
        }
    }

    /// The EZClient-style toolbar: an icon with a caption under it, which is how
    /// the original lays its actions out along the top.
    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            if theme::tool_button(
                ui,
                theme::icons::OPEN_FILE,
                "Open ROM",
                "Choose a ROM file to burn",
            )
            .clicked()
            {
                self.pick_rom();
            }
            if theme::tool_button(
                ui,
                theme::icons::DELETE,
                "Clear",
                "Forget the selected ROM file",
            )
            .clicked()
            {
                self.write_rom_path = PathBuf::new();
                self.rom_save_type = None;
            }
            if theme::tool_button(
                ui,
                theme::icons::FLASH,
                "Burn",
                "Write a ROM to the cartridge",
            )
            .clicked()
            {
                self.tab = AppTab::WriteRom;
            }
            if theme::tool_button(
                ui,
                theme::icons::DOWNLOAD,
                "Read ROM",
                "Dump the ROM from the cartridge",
            )
            .clicked()
            {
                self.tab = AppTab::ReadRom;
            }
            if theme::tool_button(
                ui,
                theme::icons::UPLOAD,
                "Write Saver",
                "Write a save file to the cartridge",
            )
            .clicked()
            {
                self.tab = AppTab::WriteSave;
            }
            if theme::tool_button(
                ui,
                theme::icons::SAVE,
                "BAK Saver",
                "Back the cartridge's save up to a file",
            )
            .clicked()
            {
                self.tab = AppTab::ReadSave;
            }
            ui.separator();
            if theme::tool_button(
                ui,
                theme::icons::REFRESH,
                "Refresh",
                "Re-scan for the writer and cartridge",
            )
            .clicked()
            {
                self.progress.clear();
                self.detect(self.tx.clone());
            }
        });
        ui.add_space(6.0);
    }

    /// Append a line to the Output list, prefixed with elapsed time.
    fn log_push(&mut self, msg: &str) {
        const MAX_LINES: usize = 500;
        let secs = self.log_start.elapsed().as_secs();
        self.log.push(format!(
            "[{:02}:{:02}:{:02}] {msg}",
            secs / 3600,
            (secs / 60) % 60,
            secs % 60
        ));
        if self.log.len() > MAX_LINES {
            let excess = self.log.len() - MAX_LINES;
            self.log.drain(..excess);
        }
    }

    fn detect(&self, tx: Sender<BgCmd>) {
        thread::spawn(move || match device::detect_mode() {
            device::DeviceMode::Bootloader => {
                let _ = tx.send(BgCmd::Status(
                    "BOOTLOADER mode (0547:2131). Click Initialize.".into(),
                ));
            }
            device::DeviceMode::Active => {
                let _ = tx.send(BgCmd::Status("ACTIVE mode (0548:1005).".into()));
                match device::read_cart_header() {
                    Ok(hdr) => {
                        let _ = tx.send(BgCmd::Header(Box::new(Some(hdr))));
                    }
                    Err(e) => {
                        let _ = tx.send(BgCmd::Error(e.to_string()));
                    }
                }
            }
            device::DeviceMode::None => {
                let _ = tx.send(BgCmd::Status("No device found. Plug in EZ-Writer.".into()));
            }
        });
    }

    /// Locate the firmware loader tables without depending on the working
    /// directory. Checks next to the executable and in the CWD; if missing,
    /// prompts the user to pick `loader_table1.bin` and derives its sibling
    /// `loader_table2.bin` from the same folder. Returns `None` if both files
    /// can't be found and the user cancels the dialog.
    fn locate_loaders() -> Option<(PathBuf, PathBuf)> {
        let t1 = device::resolve_asset("loader_table1.bin");
        let t2 = device::resolve_asset("loader_table2.bin");
        if t1.exists() && t2.exists() {
            return Some((t1, t2));
        }
        let picked = FileDialog::new()
            .set_title("Locate loader_table1.bin")
            .add_filter("Loader table", &["bin"])
            .pick_file()?;
        let dir = picked.parent().map(PathBuf::from).unwrap_or_default();
        let t1 = dir.join("loader_table1.bin");
        let t2 = dir.join("loader_table2.bin");
        (t1.exists() && t2.exists()).then_some((t1, t2))
    }

    fn show_status(&mut self, ui: &mut egui::Ui) {
        theme::page_title(ui, "Device Status");
        ui.separator();
        ui.label(&self.status_text);
        if ui
            .icon_button(theme::icons::SEARCH, "Detect Device")
            .clicked()
        {
            self.progress.clear();
            self.detect(self.tx.clone());
        }
        ui.separator();
        theme::page_title(ui, "Initialize (load firmware)");
        ui.label("Plug in device in bootloader mode, then click below:");
        if ui
            .icon_button(theme::icons::WARNING, "Initialize AN2131 (load firmware)")
            .clicked()
        {
            if let Some((t1, t2)) = Self::locate_loaders() {
                let tx = self.tx.clone();
                self.progress_value = 0.01;
                thread::spawn(move || match device::init_exact(&t1, &t2) {
                    Ok(msg) => {
                        let _ = tx.send(BgCmd::Status(msg));
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        match device::detect_mode() {
                            device::DeviceMode::Active => {
                                let _ = tx.send(BgCmd::Status("Active! Device ready.".into()));
                                if let Ok(hdr) = device::read_cart_header() {
                                    let _ = tx.send(BgCmd::Header(Box::new(Some(hdr))));
                                }
                            }
                            device::DeviceMode::Bootloader => {
                                let _ =
                                    tx.send(BgCmd::Status("Still in bootloader. Re-init?".into()));
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(BgCmd::Error(e.to_string()));
                    }
                });
            } else {
                self.progress = "Firmware loader tables not found. Place \
                    loader_table1.bin and loader_table2.bin next to the executable \
                    (or in the working directory), then try again."
                    .into();
            }
        }
        if ui
            .icon_button(theme::icons::REFRESH, "Reset Cartridge Flash")
            .clicked()
        {
            let tx = self.tx.clone();
            thread::spawn(move || match device::reset_cartridge() {
                Ok(()) => {
                    let _ = tx.send(BgCmd::Status("Cartridge reset.".into()));
                }
                Err(e) => {
                    let _ = tx.send(BgCmd::Error(e.to_string()));
                }
            });
        }
        ui.separator();
        theme::page_title(ui, "Eject");
        ui.label("Ends the cartridge session and parks the flash so the cartridge can be removed safely:");
        if ui
            .icon_button(theme::icons::EJECT, "Eject Cartridge Safely")
            .clicked()
        {
            let tx = self.tx.clone();
            self.progress = "Ejecting...".into();
            thread::spawn(move || match device::eject_safely() {
                Ok(msg) => {
                    let _ = tx.send(BgCmd::Status(msg));
                    let _ = tx.send(BgCmd::Header(Box::new(None)));
                }
                Err(e) => {
                    let _ = tx.send(BgCmd::Error(e.to_string()));
                }
            });
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(2));
        ui.separator();
        // Detect / Initialize / Reset report failures through `progress`; without
        // this the Device tab looked like it did nothing when they failed.
        ui.label(&self.progress);
    }

    fn show_cart_info(&mut self, ui: &mut egui::Ui) {
        theme::page_title(ui, "Cartridge Information");
        if ui
            .icon_button(theme::icons::CARTRIDGE, "Detect Cartridge")
            .clicked()
        {
            let tx = self.tx.clone();
            self.progress_value = 0.01;
            thread::spawn(move || match device::read_cart_header() {
                Ok(hdr) => {
                    let _ = tx.send(BgCmd::Header(Box::new(Some(hdr))));
                    let _ = tx.send(BgCmd::Status("ACTIVE mode (0548:1005).".into()));
                }
                Err(e) => {
                    let _ = tx.send(BgCmd::Error(e.to_string()));
                }
            });
        }
        ui.separator();

        if let Some(ref hdr) = self.cart_header {
            // The cartridge's box art, when one could be matched.
            if let Some(tex) = &self.banner {
                let native = tex.size_vec2();
                let target_h = 200.0_f32;
                let size = if native.y > 0.0 {
                    egui::vec2(native.x * (target_h / native.y), target_h)
                } else {
                    native
                };
                ui.add(egui::Image::new(egui::load::SizedTexture::new(
                    tex.id(),
                    size,
                )));
                ui.add_space(6.0);
            } else if !self.banner_status.is_empty() {
                ui.label(&self.banner_status);
                ui.add_space(6.0);
            }
            ui.horizontal(|ui| {
                // The Nintendo logo is identical for every GBA game, so once
                // there is real box art it is just noise.
                if self.banner.is_none() && !self.nintendo_logo.is_empty() {
                    let size = egui::Vec2::new(52.0 * 5.0, 24.0 * 5.0);
                    let (response, painter) = ui.allocate_painter(size, egui::Sense::hover());
                    let origin = response.rect.min;
                    let pixel_size = egui::Vec2::new(5.0, 5.0);
                    for (i, &rgb) in self.nintendo_logo.iter().enumerate() {
                        let row = i / 52;
                        let col = i % 52;
                        let pos = origin + egui::vec2(col as f32 * 5.0, row as f32 * 5.0);
                        let color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                        painter.rect_filled(egui::Rect::from_min_size(pos, pixel_size), 0.0, color);
                    }
                }
                ui.vertical(|ui| {
                    ui.heading(&hdr.title);
                    ui.label(format!("Game ID: {}", hdr.code));
                    if let Some(db) = device::lookup_game(&hdr.code) {
                        ui.label(format!("Known as: {}", db.title));
                    }
                    ui.label(format!("Maker: {}", hdr.maker));
                    ui.label(format!("Save type: {}", hdr.save_type));
                    let sz = device::save_size_bytes(&hdr.save_type);
                    ui.label(format!("Save size: {} KB ({} bytes)", sz / 1024, sz));
                });
            });
        } else {
            ui.label("No cartridge detected. Click 'Detect Cartridge'.");
        }
        ui.separator();
        ui.label(&self.progress);
        // The box-art fetch runs on a worker thread; keep repainting until it
        // lands, otherwise the message is never consumed on this tab.
        if !self.banner_status.is_empty() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(300));
        }
    }

    fn show_read_rom(&mut self, ui: &mut egui::Ui) {
        theme::page_title(ui, "Read ROM to File");
        ui.horizontal_wrapped(|ui| {
            if ui
                .icon_button(theme::icons::OPEN_FILE, "Select File...")
                .clicked()
                && let Some(path) = FileDialog::new()
                    .set_title("Save GBA ROM As")
                    .add_filter("GBA ROM", &["gba", "bin"])
                    .save_file()
            {
                self.rom_path = path;
            }
            ui.label(self.rom_path.display().to_string());
        });
        if let Some(ref hdr) = self.cart_header {
            let rom_size = hdr.rom_size;
            ui.checkbox(
                &mut self.confirm_chunks,
                "Verify every chunk (reads each block twice; slower, catches unstable cartridges)",
            );
            let dump_label = format!("[v] Dump ROM ({:.0} MB)", rom_size as f64 / 1_048_576.0);
            if !self.rom_path.as_os_str().is_empty() && ui.button(&dump_label).clicked() {
                let path = self.rom_path.clone();
                let tx = self.tx.clone();
                let total = rom_size as u64;
                let confirm = self.confirm_chunks;
                self.progress_value = 0.01;
                thread::spawn(move || match device::CartSession::open() {
                    Ok(session) => {
                        let result =
                            session.dump_rom_stream(&path, total, 0, confirm, |written, total| {
                                let _ = tx.send(BgCmd::DumpProgress {
                                    bytes_read: written,
                                    total_bytes: total,
                                });
                                Ok(())
                            });
                        match result {
                            Ok(()) => {
                                let _ = tx.send(BgCmd::Progress(format!(
                                    "[OK] ROM dump: {} MB",
                                    total / (1024 * 1024)
                                )));
                                let _ = tx.send(BgCmd::DumpProgress {
                                    bytes_read: total,
                                    total_bytes: total,
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(BgCmd::Error(e.to_string()));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(BgCmd::Error(format!("Failed to open cart session: {e}")));
                    }
                });
            }
        } else {
            ui.label("(!) Detect cartridge first (Cart Info tab)");
        }
        if self.progress_value > 0.0 {
            ui.add(
                egui::ProgressBar::new(self.progress_value)
                    .show_percentage()
                    .animate(true),
            );
        }
        ui.separator();
        ui.label(&self.progress);
        ui.ctx().request_repaint();
    }

    fn show_write_rom(&mut self, ui: &mut egui::Ui) {
        theme::page_title(ui, "Write ROM to Cartridge (Burn)");
        ui.colored_label(
            theme::DANGER,
            "[!]  DESTRUCTIVE — this ERASES and rewrites cartridge flash. Back it up first.",
        );
        ui.separator();

        ui.horizontal_wrapped(|ui| {
            if ui
                .icon_button(theme::icons::OPEN_FILE, "Select ROM File...")
                .clicked()
                && let Some(path) = FileDialog::new()
                    .set_title("Open GBA ROM")
                    .add_filter("GBA ROM", &["gba", "bin"])
                    .add_filter("All Files", &["*"])
                    .pick_file()
            {
                self.set_rom(path);
            }
            ui.label(self.write_rom_path.display().to_string());
        });

        if !self.write_rom_path.as_os_str().is_empty() {
            let size = std::fs::metadata(&self.write_rom_path)
                .map(|m| m.len())
                .unwrap_or(0);
            ui.label(format!("File size: {size} bytes ({} KB)", size / 1024));
            if let Some(w) = &self.wrapped_from {
                ui.colored_label(
                    theme::ACCENT,
                    format!("Game Boy ROM wrapped with Goomba: {w}"),
                );
            }
            match self.rom_save_type {
                Some(t) => {
                    ui.label(format!(
                        "Save hardware: {t} ({} KB), from the ROM's SDK save-library marker",
                        device::save_size_bytes(t) / 1024
                    ));
                }
                None => {
                    ui.label("Save hardware: no SDK save-library marker found in this ROM");
                }
            }
        }

        ui.horizontal(|ui| {
            ui.label("Start address:");
            ui.add(egui::TextEdit::singleline(&mut self.write_rom_addr).desired_width(110.0));
            ui.label("Inter-chunk delay (ms):");
            ui.add(egui::DragValue::new(&mut self.write_rom_delay_ms).range(0..=1000));
        });
        // Wrapped, not a plain horizontal row: four checkboxes plus the IPS
        // controls are wider than the content pane, and a plain row pushes the
        // overflow off the right edge where it cannot be clicked.
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut self.write_rom_init,
                "Run CPLD/bank init first (EZClient sequence)",
            );
            ui.checkbox(
                &mut self.write_rom_no_erase,
                "Skip erase (only if the region is already blank)",
            );
            ui.checkbox(&mut self.write_rom_verify, "Verify after writing");
            ui.checkbox(
                &mut self.write_rom_trim,
                "Trim ROM (strip trailing 0xFF/0x00 padding before writing)",
            );
            if ui
                .icon_button(theme::icons::OPEN_FILE, "Select IPS Patch...")
                .clicked()
                && let Some(path) = FileDialog::new()
                    .set_title("Open IPS Patch")
                    .add_filter("IPS patch", &["ips"])
                    .add_filter("All Files", &["*"])
                    .pick_file()
            {
                self.ips_path = path;
            }
            if self.ips_path.as_os_str().is_empty() {
                ui.label("(no IPS patch applied)");
            } else {
                ui.label(format!("IPS: {}", self.ips_path.display()));
                if ui.button("Clear").clicked() {
                    self.ips_path = PathBuf::new();
                }
            }
        });
        ui.separator();

        let addr = parse_hex_u32(&self.write_rom_addr);
        if self.write_rom_path.as_os_str().is_empty() {
            ui.label("Select a ROM file to enable writing.");
        } else if addr.is_none() {
            ui.colored_label(theme::DANGER, "Start address must be hex, e.g. 0x000000.");
        } else if ui
            .icon_button(theme::icons::FLASH, "ERASE + WRITE ROM")
            .clicked()
        {
            let path = self.write_rom_path.clone();
            let opts = device::RomWriteOptions {
                byte_addr: addr.unwrap(),
                delay_ms: self.write_rom_delay_ms,
                no_erase: self.write_rom_no_erase,
                verify: self.write_rom_verify,
                init: self.write_rom_init,
            };
            let tx = self.tx.clone();
            let trim = self.write_rom_trim;
            let ips_path = self.ips_path.clone();
            self.progress_value = 0.01;
            thread::spawn(move || {
                let mut data = match std::fs::read(&path) {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = tx.send(BgCmd::Error(e.to_string()));
                        return;
                    }
                };
                if trim {
                    let keep = device::trim_rom_padding(&data).len();
                    let _ = tx.send(BgCmd::Status(format!(
                        "Trim ROM: {} -> {keep} bytes",
                        data.len()
                    )));
                    data.truncate(keep);
                }
                // Patch after trimming so patched bytes are never trimmed away.
                if !ips_path.as_os_str().is_empty() {
                    let patch = match std::fs::read(&ips_path) {
                        Ok(p) => p,
                        Err(e) => {
                            let _ = tx.send(BgCmd::Error(format!("reading IPS patch: {e}")));
                            return;
                        }
                    };
                    match device::apply_ips(&mut data, &patch) {
                        Ok(summary) => {
                            let _ = tx.send(BgCmd::Status(summary));
                        }
                        Err(e) => {
                            let _ = tx.send(BgCmd::Error(format!("IPS patch: {e}")));
                            return;
                        }
                    }
                }
                let total = data.len() as u64;
                match device::write_rom(&data, &opts, |written, tot| {
                    let _ = tx.send(BgCmd::RomWriteProgress {
                        bytes_written: written,
                        total_bytes: tot,
                    });
                }) {
                    Ok(msg) => {
                        let _ = tx.send(BgCmd::RomWriteProgress {
                            bytes_written: total,
                            total_bytes: total,
                        });
                        let _ = tx.send(BgCmd::Progress(msg));
                    }
                    Err(e) => {
                        let _ = tx.send(BgCmd::Error(e.to_string()));
                    }
                }
            });
        }

        if self.progress_value > 0.0 {
            ui.add(
                egui::ProgressBar::new(self.progress_value)
                    .show_percentage()
                    .animate(true),
            );
        }
        ui.separator();
        ui.label(&self.progress);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(300));
    }

    fn show_read_save(&mut self, ui: &mut egui::Ui) {
        theme::page_title(ui, "Read Save to File");
        if let Some(ref hdr) = self.cart_header {
            let sz = device::save_size_bytes(&hdr.save_type);
            if device::is_known_save_type(&hdr.save_type) {
                ui.label(format!(
                    "Detected: {} -> {} save ({} KB)",
                    hdr.title,
                    hdr.save_type,
                    sz / 1024
                ));
            } else {
                ui.colored_label(
                    theme::WARN,
                    format!(
                        "Detected: {} — game code '{}' is not in the built-in database, so the save \
                         chip type is unknown. Reading is disabled rather than guessed. Run \
                         `ezwriter-cli save-id` to identify the chip.",
                        hdr.title, hdr.code
                    ),
                );
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .icon_button(theme::icons::OPEN_FILE, "Select File...")
                .clicked()
                && let Some(path) = FileDialog::new()
                    .set_title("Save Save As")
                    .add_filter("GBA Save", &["sav", "bin"])
                    .save_file()
            {
                self.save_path = path;
            }
            ui.label(self.save_path.display().to_string());
        });
        if !self.save_path.as_os_str().is_empty()
            && ui.icon_button(theme::icons::SAVE, "Dump Save").clicked()
        {
            let path = self.save_path.clone();
            let tx = self.tx.clone();
            let save_type = self.effective_save_type();
            let confirm = self.confirm_chunks;
            // Show the progress bar immediately; the worker drives it via
            // SaveReadProgress messages.
            self.progress_value = 0.01;
            thread::spawn(move || {
                if !device::is_known_save_type(&save_type) {
                    let _ = tx.send(BgCmd::Error(format!(
                        "Game code not recognised, so the save chip type is unknown ('{save_type}'). \
                         Refusing to guess — the fallback path can lock the cartridge CPLD until the \
                         device is replugged. Run `ezwriter-cli save-id` to identify the chip, then \
                         `ezwriter-cli save-read -t f|s|e --output <file>`."
                    )));
                    return;
                }
                let sz = device::save_size_bytes(&save_type);
                // Genuine 128KB GBA FLASH (e.g. Pokémon Gen 3) needs the native
                // two-bank reader; other types use the generic per-block read.
                let all = if save_type.contains("FLASH") && sz == 128 * 1024 {
                    let txp = tx.clone();
                    match device::read_flash128_save(confirm, move |read, tot| {
                        let _ = txp.send(BgCmd::SaveReadProgress {
                            bytes_read: read,
                            total_bytes: tot,
                        });
                    }) {
                        Ok(data) => data,
                        Err(e) => {
                            let _ = tx.send(BgCmd::Error(e.to_string()));
                            return;
                        }
                    }
                } else {
                    let mut all = Vec::with_capacity(sz);
                    let mut failure: Option<(u32, String)> = None;
                    for offset in (0..sz as u32).step_by(0x1000) {
                        match device::read_save_with_type(offset, 64, &save_type, confirm) {
                            Ok(data) => all.extend(data),
                            Err(e) => {
                                // Breaking out here used to write whatever had
                                // been read so far as if it were the whole save.
                                failure = Some((offset, e.to_string()));
                                break;
                            }
                        }
                        let _ = tx.send(BgCmd::SaveReadProgress {
                            bytes_read: all.len() as u64,
                            total_bytes: sz as u64,
                        });
                    }
                    if let Some((offset, e)) = failure {
                        let _ = tx.send(BgCmd::Error(format!(
                            "save read aborted at offset 0x{offset:X} after {} of {sz} bytes: {e}. \
                             No file was written.",
                            all.len()
                        )));
                        return;
                    }
                    all
                };
                // A Goomba container is neither a Game Boy save nor a Gen 3
                // save: pull the GB/GBC SRAM out of it instead of validating.
                if device::goomba_is_save(&all) {
                    let _ = tx.send(BgCmd::Progress(format!(
                        "Goomba save detected ({} game record(s))",
                        device::goomba_scan_saves(&all).len()
                    )));
                    match device::dump_goomba_saves(&path, &all) {
                        Ok(msg) => {
                            let _ = tx.send(BgCmd::Progress(msg));
                        }
                        Err(e) => {
                            let _ = tx.send(BgCmd::Error(e.to_string()));
                        }
                    }
                    return;
                }
                if let Err(e) = device::validate_save_dump(&all, &save_type) {
                    let _ = tx.send(BgCmd::Error(e.to_string()));
                    return;
                }
                if let Err(e) = device::dump_to_file(&path, &all) {
                    let _ = tx.send(BgCmd::Error(e.to_string()));
                } else {
                    let _ = tx.send(BgCmd::Progress(format!(
                        "[OK] Save dump: {} bytes",
                        all.len()
                    )));
                }
            });
        }
        if self.progress_value > 0.0 {
            ui.add(
                egui::ProgressBar::new(self.progress_value)
                    .show_percentage()
                    .animate(true),
            );
        }
        ui.separator();
        ui.label(&self.progress);
        ui.ctx().request_repaint();
    }

    fn show_write_save(&mut self, ui: &mut egui::Ui) {
        theme::page_title(ui, "Write Save to Cartridge");
        ui.colored_label(theme::DANGER, "[!]  WRITE OPERATION — USE WITH CAUTION");
        ui.separator();
        if let Some(ref hdr) = self.cart_header {
            ui.label(format!("Current cart: {} [{}]", hdr.title, hdr.code));
            let effective = self.effective_save_type();
            if device::is_known_save_type(&hdr.save_type) {
                ui.label(format!("Save type: {}", hdr.save_type));
            } else if let Some(t) = self.rom_save_type {
                ui.label(format!(
                    "Save type: {t} — from the selected ROM's SDK save-library marker \
                     (the cartridge's game code is not in the database)"
                ));
            } else {
                ui.label(format!("Save type: {effective}"));
            }
        } else {
            ui.label("(!) No cartridge detected — detect in Cart Info tab first");
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .icon_button(theme::icons::OPEN_FILE, "Select Save File...")
                .clicked()
                && let Some(path) = FileDialog::new()
                    .set_title("Open Save File")
                    .add_filter("GBA Save", &["sav", "bin"])
                    .add_filter("All Files", &["*"])
                    .pick_file()
            {
                self.save_path = path;
            }
            ui.label(self.save_path.display().to_string());
        });
        if !self.save_path.as_os_str().is_empty() && self.cart_header.is_some() {
            ui.separator();
            if ui
                .icon_button(theme::icons::UPLOAD, "Write Save to Cartridge")
                .clicked()
            {
                let path = self.save_path.clone();
                let tx = self.tx.clone();
                let save_type = self.effective_save_type();
                thread::spawn(move || {
                    let data = match std::fs::read(&path) {
                        Ok(d) => d,
                        Err(e) => {
                            let _ = tx.send(BgCmd::Error(e.to_string()));
                            return;
                        }
                    };
                    let total = data.len() as u64;
                    match device::write_save(&data, &save_type, |read, tot| {
                        let _ = tx.send(BgCmd::SaveWriteProgress {
                            bytes_read: read,
                            total_bytes: tot,
                        });
                    }) {
                        Ok(msg) => {
                            let _ = tx.send(BgCmd::SaveWriteProgress {
                                bytes_read: total,
                                total_bytes: total,
                            });
                            let _ = tx.send(BgCmd::Progress(msg));
                        }
                        Err(e) => {
                            let _ = tx.send(BgCmd::Error(e.to_string()));
                        }
                    }
                });
            }
        }
        if self.progress_value > 0.0 {
            ui.add(
                egui::ProgressBar::new(self.progress_value)
                    .show_percentage()
                    .animate(true),
            );
        }
        ui.separator();
        ui.label(&self.progress);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_size_flash_128k() {
        assert_eq!(device::save_size_bytes("FLASH 128K"), 128 * 1024);
    }

    #[test]
    fn save_size_sram_256k() {
        assert_eq!(device::save_size_bytes("SRAM 256K"), 256 * 1024);
    }

    #[test]
    fn save_size_sram_64k() {
        assert_eq!(device::save_size_bytes("SRAM 64K"), 64 * 1024);
    }

    #[test]
    fn save_size_eeprom_8k() {
        assert_eq!(device::save_size_bytes("EEPROM 8K"), 8 * 1024);
    }

    #[test]
    fn save_size_eeprom_512() {
        assert_eq!(device::save_size_bytes("EEPROM 512"), 512);
    }

    #[test]
    fn save_size_unknown_defaults_32k() {
        assert_eq!(device::save_size_bytes("UNKNOWN"), 32 * 1024);
    }
}
