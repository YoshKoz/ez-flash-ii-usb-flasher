use eframe::egui;
use rfd::FileDialog;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::device;

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

fn save_size_bytes(save_type: &str) -> usize {
    if save_type.contains("128K") || save_type.contains("256K") {
        if save_type.contains("256K") {
            256 * 1024
        } else {
            128 * 1024
        }
    } else if save_type.contains("64K") {
        64 * 1024
    } else if save_type.contains("8K") || save_type.contains("8k") {
        8 * 1024
    } else if save_type.contains("512") {
        512
    } else {
        32 * 1024
    }
}

enum AppTab {
    Status,
    CartInfo,
    ReadRom,
    ReadSave,
    WriteSave,
}

enum BgCmd {
    Status(String),
    Header(Box<Option<device::CartHeader>>),
    Progress(String),
    DumpProgress { bytes_read: u64, total_bytes: u64 },
    Error(String),
}

pub struct EzWriterApp {
    tab: AppTab,
    status_text: String,
    cart_header: Option<device::CartHeader>,
    nintendo_logo: Vec<[u8; 3]>,
    rom_path: PathBuf,
    save_path: PathBuf,
    progress: String,
    progress_value: f32,
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
            rom_path: PathBuf::new(),
            save_path: PathBuf::new(),
            progress: String::new(),
            progress_value: 0.0,
            tx,
            rx,
        }
    }
}

impl eframe::App for EzWriterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                BgCmd::Status(s) => self.status_text = s,
                BgCmd::Header(h) => {
                    self.cart_header = *h;
                    if let Some(ref hdr) = self.cart_header {
                        self.progress = format!("Cartridge: {} [{}]", hdr.title, hdr.code);
                        self.nintendo_logo = decode_nintendo_logo(&hdr.raw_header[4..160]);
                    }
                }
                BgCmd::Progress(s) => {
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
                BgCmd::Error(e) => {
                    self.progress = format!("Error: {e}");
                    self.cart_header = None;
                    self.nintendo_logo.clear();
                }
            }
        }

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("EZ-Writer II FlashGBX");
                ui.separator();
                if ui
                    .selectable_label(matches!(self.tab, AppTab::Status), "Status")
                    .clicked()
                {
                    self.tab = AppTab::Status;
                }
                if ui
                    .selectable_label(matches!(self.tab, AppTab::CartInfo), "Cart Info")
                    .clicked()
                {
                    self.tab = AppTab::CartInfo;
                }
                if ui
                    .selectable_label(matches!(self.tab, AppTab::ReadRom), "Read ROM")
                    .clicked()
                {
                    self.tab = AppTab::ReadRom;
                }
                if ui
                    .selectable_label(matches!(self.tab, AppTab::ReadSave), "Read Save")
                    .clicked()
                {
                    self.tab = AppTab::ReadSave;
                }
                if ui
                    .selectable_label(matches!(self.tab, AppTab::WriteSave), "Write Save")
                    .clicked()
                {
                    self.tab = AppTab::WriteSave;
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            AppTab::Status => self.show_status(ui, ctx),
            AppTab::CartInfo => self.show_cart_info(ui),
            AppTab::ReadRom => self.show_read_rom(ui, ctx),
            AppTab::ReadSave => self.show_read_save(ui, ctx),
            AppTab::WriteSave => self.show_write_save(ui),
        });
    }
}

impl EzWriterApp {
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

    fn show_status(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Device Status");
        ui.separator();
        ui.label(&self.status_text);
        if ui.button("[R] Detect Device").clicked() {
            self.progress.clear();
            self.detect(self.tx.clone());
        }
        ui.separator();
        ui.heading("Initialize (load firmware)");
        ui.label("Plug in device in bootloader mode, then click below:");
        if ui.button("[!] Initialize AN2131 (load firmware)").clicked() {
            let tx = self.tx.clone();
            let t1 = PathBuf::from("loader_table1.bin");
            let t2 = PathBuf::from("loader_table2.bin");
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
                            let _ = tx.send(BgCmd::Status("Still in bootloader. Re-init?".into()));
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    let _ = tx.send(BgCmd::Error(e.to_string()));
                }
            });
        }
        if ui.button("[R] Reset Cartridge Flash").clicked() {
            let tx = self.tx.clone();
            thread::spawn(move || {
                match device::find_device(device::EZWRITER_VID, device::EZWRITER_PID) {
                    Ok((dev, _)) => {
                        if let Ok(handle) = dev.open() {
                            device::reset_jedec(&handle);
                        }
                        let _ = tx.send(BgCmd::Status("Cartridge reset.".into()));
                    }
                    Err(_) => {
                        let _ = tx.send(BgCmd::Status("Not in active mode.".into()));
                    }
                }
            });
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
    }

    fn show_cart_info(&mut self, ui: &mut egui::Ui) {
        ui.heading("Cartridge Information");
        if ui.button("[?] Detect Cartridge").clicked() {
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
            ui.horizontal(|ui| {
                if !self.nintendo_logo.is_empty() {
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
                    let sz = save_size_bytes(&hdr.save_type);
                    ui.label(format!("Save size: {} KB ({} bytes)", sz / 1024, sz));
                });
            });
        } else {
            ui.label("No cartridge detected. Click 'Detect Cartridge'.");
        }
        ui.separator();
        ui.label(&self.progress);
    }

    fn show_read_rom(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Read ROM to File");
        ui.horizontal(|ui| {
            if ui.button("[..] Select File...").clicked()
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
            let dump_label = format!("[v] Dump ROM ({:.0} MB)", rom_size as f64 / 1_048_576.0);
            if !self.rom_path.as_os_str().is_empty() && ui.button(&dump_label).clicked() {
                let path = self.rom_path.clone();
                let tx = self.tx.clone();
                let total = rom_size as u64;
                self.progress_value = 0.01;
                thread::spawn(move || match device::CartSession::open() {
                    Ok(session) => {
                        let result = session.dump_rom_stream(&path, total, 0, |written, total| {
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
        ctx.request_repaint();
    }

    fn show_read_save(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Read Save to File");
        if let Some(ref hdr) = self.cart_header {
            let sz = save_size_bytes(&hdr.save_type);
            ui.label(format!(
                "Detected: {} → {} save ({} KB)",
                hdr.title,
                hdr.save_type,
                sz / 1024
            ));
        }
        ui.horizontal(|ui| {
            if ui.button("[..] Select File...").clicked()
                && let Some(path) = FileDialog::new()
                    .set_title("Save Save As")
                    .add_filter("GBA Save", &["sav", "bin"])
                    .save_file()
            {
                self.save_path = path;
            }
            ui.label(self.save_path.display().to_string());
        });
        if !self.save_path.as_os_str().is_empty() && ui.button("[v] Dump Save").clicked() {
            let path = self.save_path.clone();
            let tx = self.tx.clone();
            let save_type = self
                .cart_header
                .as_ref()
                .map_or("FLASH 128K".to_string(), |h| h.save_type.clone());
            thread::spawn(move || {
                let sz = save_size_bytes(&save_type);
                let mut all = Vec::with_capacity(sz);
                for offset in (0..sz as u32).step_by(0x1000) {
                    match device::read_save(offset, 64) {
                        Ok(data) => all.extend(data),
                        Err(_) => break,
                    }
                    if all.len() % (64 * 1024) == 0 && !all.is_empty() {
                        let _ = tx.send(BgCmd::Progress(format!(
                            "[v] Dumping save... {} / {} KB",
                            all.len() / 1024,
                            sz / 1024
                        )));
                    }
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
        ctx.request_repaint();
    }

    fn show_write_save(&mut self, ui: &mut egui::Ui) {
        ui.heading("Write Save to Cartridge");
        ui.colored_label(
            egui::Color32::RED,
            "[!]  WRITE OPERATION — USE WITH CAUTION",
        );
        ui.separator();
        if let Some(ref hdr) = self.cart_header {
            ui.label(format!("Current cart: {} [{}]", hdr.title, hdr.code));
            ui.label(format!("Save type: {}", hdr.save_type));
        }
        ui.horizontal(|ui| {
            if ui.button("[..] Select Save File...").clicked()
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
        if !self.save_path.as_os_str().is_empty() {
            ui.separator();
            ui.colored_label(egui::Color32::YELLOW, "Write save not yet implemented.");
            ui.label("Requires reverse engineering the erase+program protocol from EZClient.exe.");
        }
        ui.separator();
        ui.label(&self.progress);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_size_flash_128k() {
        assert_eq!(save_size_bytes("FLASH 128K"), 128 * 1024);
    }

    #[test]
    fn save_size_sram_256k() {
        assert_eq!(save_size_bytes("SRAM 256K"), 256 * 1024);
    }

    #[test]
    fn save_size_sram_64k() {
        assert_eq!(save_size_bytes("SRAM 64K"), 64 * 1024);
    }

    #[test]
    fn save_size_eeprom_8k() {
        assert_eq!(save_size_bytes("EEPROM 8K"), 8 * 1024);
    }

    #[test]
    fn save_size_eeprom_512() {
        assert_eq!(save_size_bytes("EEPROM 512"), 512);
    }

    #[test]
    fn save_size_unknown_defaults_32k() {
        assert_eq!(save_size_bytes("UNKNOWN"), 32 * 1024);
    }
}
