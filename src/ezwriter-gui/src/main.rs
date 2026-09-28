mod app;
mod device;
mod theme;

pub const BUILD_STAMP: &str = "0.1.0";

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([900.0, 580.0])
            // The app draws its own Fluent title bar, so the OS frame is off.
            // That means move/resize have to be handled in-app (see
            // EzWriterApp::show_title_bar and resize_edges).
            .with_decorations(false)
            .with_resizable(true)
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
