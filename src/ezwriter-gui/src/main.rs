mod app;
mod device;

pub const BUILD_STAMP: &str = "0.1.0";

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 480.0])
            .with_title(format!("EZ-Flash II USB Flasher {}", BUILD_STAMP)),
        ..Default::default()
    };
    eframe::run_native(
        &format!("EZ-Flash II USB Flasher {}", BUILD_STAMP),
        options,
        Box::new(|_cc| Ok(Box::new(app::EzWriterApp::default()))),
    )
}
