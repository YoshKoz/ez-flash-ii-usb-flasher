mod app;
mod device;
mod theme;

pub const BUILD_STAMP: &str = "0.1.0";

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([880.0, 560.0])
            .with_title(format!("EZ-Flash II USB Flasher {}", BUILD_STAMP)),
        ..Default::default()
    };
    eframe::run_native(
        &format!("EZ-Flash II USB Flasher {}", BUILD_STAMP),
        options,
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(app::EzWriterApp::default()))
        }),
    )
}
