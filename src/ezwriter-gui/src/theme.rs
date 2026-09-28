//! Fluent (WinUI 3) look for the GUI.
//!
//! The typefaces are the ones Windows already ships — **Segoe UI Variable** for
//! text and **Segoe Fluent Icons** for glyphs — so the app looks native without
//! bundling any font assets. Segoe UI and Segoe MDL2 Assets are used instead
//! when the Windows 11 files are missing, which keeps Windows 10 working.
//!
//! Colours follow the Fluent dark palette so the app sits alongside WinUI apps
//! (DLSS Swapper, Terminal, Explorer) rather than looking like a 2006 MFC tool.

use eframe::egui;
use std::sync::Arc;

/// Font family holding the Fluent icon glyphs.
pub const ICON_FAMILY: &str = "fluent_icons";

/// Glyph codepoints shared by Segoe Fluent Icons and Segoe MDL2 Assets.
pub mod icons {
    /// Open a file (folder with arrow).
    pub const OPEN_FILE: &str = "\u{E8E5}";
    /// Delete / clear.
    pub const DELETE: &str = "\u{E74D}";
    /// Lightning bolt, used for the burn/flash action.
    pub const FLASH: &str = "\u{E945}";
    /// Download arrow — reading off the cartridge.
    pub const DOWNLOAD: &str = "\u{E896}";
    /// Upload arrow — writing to the cartridge.
    pub const UPLOAD: &str = "\u{E898}";
    /// Floppy/save.
    pub const SAVE: &str = "\u{E74E}";
    /// Circular refresh arrows.
    pub const REFRESH: &str = "\u{E72C}";
    /// Microchip/processor, for the device section.
    pub const CHIP: &str = "\u{E964}";
    /// Memory/cartridge.
    pub const CARTRIDGE: &str = "\u{E7B8}";
    /// Magnifying glass.
    pub const SEARCH: &str = "\u{E721}";
    /// Warning triangle.
    pub const WARNING: &str = "\u{E7BA}";
    /// Eject.
    pub const EJECT: &str = "\u{E7B4}";
    /// Window controls.
    pub const MINIMIZE: &str = "\u{E921}";
    pub const MAXIMIZE: &str = "\u{E922}";
    pub const RESTORE: &str = "\u{E923}";
    pub const CLOSE: &str = "\u{E8BB}";
}

// Fluent dark palette.
pub const BG: egui::Color32 = egui::Color32::from_rgb(0x20, 0x20, 0x20);
pub const LAYER: egui::Color32 = egui::Color32::from_rgb(0x27, 0x27, 0x27);
pub const CARD: egui::Color32 = egui::Color32::from_rgb(0x2B, 0x2B, 0x2B);
pub const CONTROL: egui::Color32 = egui::Color32::from_rgb(0x2D, 0x2D, 0x2D);
pub const CONTROL_HOVER: egui::Color32 = egui::Color32::from_rgb(0x38, 0x38, 0x38);
pub const CONTROL_PRESSED: egui::Color32 = egui::Color32::from_rgb(0x25, 0x25, 0x25);
pub const STROKE: egui::Color32 = egui::Color32::from_rgb(0x3A, 0x3A, 0x3A);
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(0xC8, 0xC8, 0xC8);
/// Fluent dark accent ("AccentFillColorDefault" for the default blue).
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x4C, 0xC2, 0xFF);
pub const DANGER: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x99, 0xA4);
pub const WARN: egui::Color32 = egui::Color32::from_rgb(0xFC, 0xE1, 0x00);

/// A font id for the icon family at the given size.
pub fn icon_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(ICON_FAMILY.into()))
}

fn load_first(paths: &[&str]) -> Option<Vec<u8>> {
    paths
        .iter()
        .find_map(|p| std::fs::read(p).ok())
        .filter(|b| !b.is_empty())
}

/// Load the Segoe fonts and apply the Fluent style. Call once at startup.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Windows 11 first, Windows 10 fallbacks second.
    if let Some(bytes) = load_first(&[
        r"C:\Windows\Fonts\SegUIVar.ttf",
        r"C:\Windows\Fonts\segoeui.ttf",
    ]) {
        fonts.font_data.insert(
            "segoe_ui".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe_ui".to_owned());
    }

    if let Some(bytes) = load_first(&[
        r"C:\Windows\Fonts\SegoeIcons.ttf",
        r"C:\Windows\Fonts\segmdl2.ttf",
    ]) {
        fonts.font_data.insert(
            "fluent_icons".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts.families.insert(
            egui::FontFamily::Name(ICON_FAMILY.into()),
            vec!["fluent_icons".to_owned()],
        );
    }

    ctx.set_fonts(fonts);
    apply_visuals(ctx);
}

fn apply_visuals(ctx: &egui::Context) {
    // `all_styles_mut` covers both the light and dark style sets; egui 0.36 has
    // no single style to mutate any more.
    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;

        v.dark_mode = true;
        v.panel_fill = BG;
        v.window_fill = LAYER;
        v.faint_bg_color = CARD;
        v.extreme_bg_color = egui::Color32::from_rgb(0x1B, 0x1B, 0x1B);
        v.override_text_color = Some(TEXT);
        v.hyperlink_color = ACCENT;
        v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
        v.selection.stroke = egui::Stroke::new(1.0, ACCENT);

        let radius = egui::CornerRadius::same(4);
        let stroke = egui::Stroke::new(1.0, STROKE);

        v.widgets.noninteractive.bg_fill = LAYER;
        v.widgets.noninteractive.weak_bg_fill = LAYER;
        v.widgets.noninteractive.bg_stroke = stroke;
        v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_DIM);
        v.widgets.noninteractive.corner_radius = radius;

        v.widgets.inactive.bg_fill = CONTROL;
        v.widgets.inactive.weak_bg_fill = CONTROL;
        v.widgets.inactive.bg_stroke = stroke;
        v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
        v.widgets.inactive.corner_radius = radius;

        v.widgets.hovered.bg_fill = CONTROL_HOVER;
        v.widgets.hovered.weak_bg_fill = CONTROL_HOVER;
        v.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(0x50, 0x50, 0x50));
        v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
        v.widgets.hovered.corner_radius = radius;

        v.widgets.active.bg_fill = CONTROL_PRESSED;
        v.widgets.active.weak_bg_fill = CONTROL_PRESSED;
        v.widgets.active.bg_stroke = stroke;
        v.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT);
        v.widgets.active.corner_radius = radius;

        v.widgets.open.bg_fill = LAYER;
        v.widgets.open.weak_bg_fill = LAYER;
        v.widgets.open.bg_stroke = stroke;
        v.widgets.open.fg_stroke = egui::Stroke::new(1.0, TEXT);
        v.widgets.open.corner_radius = radius;

        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.visuals.window_corner_radius = egui::CornerRadius::same(8);
    });
}

/// A button label that pairs a Fluent glyph with its caption.
///
/// The glyph and the text need different font families, which a plain string
/// cannot express, so this builds a `LayoutJob` with one section per font.
pub fn icon_text(glyph: &str, text: &str) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        glyph,
        0.0,
        egui::TextFormat {
            font_id: icon_font(15.0),
            color: TEXT,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.append(
        &format!("   {text}"),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(14.0),
            color: TEXT,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.into()
}

/// A WinUI-style card: a rounded surface with a hairline border and padding,
/// used to group a section's controls the way Settings cards do.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::NONE
        .fill(CARD)
        .stroke(egui::Stroke::new(1.0, STROKE))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Fluent page title, for the top of a section.
pub fn page_title(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(20.0).strong());
}

/// A Fluent window-control button (the title bar's minimise/maximise/close).
pub fn window_button(ui: &mut egui::Ui, glyph: &str, name: &str, is_close: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(46.0, 32.0), egui::Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter().clone();

    if hovered {
        let fill = if is_close {
            egui::Color32::from_rgb(0xC4, 0x2B, 0x1C)
        } else {
            CONTROL_HOVER
        };
        painter.rect_filled(rect, egui::CornerRadius::ZERO, fill);
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        icon_font(11.0),
        if hovered && is_close {
            egui::Color32::WHITE
        } else {
            TEXT
        },
    );

    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
    response.on_hover_text(name)
}

/// Adds `Ui::icon_button`, a button whose face is a Fluent glyph plus a caption.
pub trait IconButtonExt {
    /// Draw a glyph + caption button.
    fn icon_button(&mut self, glyph: &str, text: &str) -> egui::Response;
}

impl IconButtonExt for egui::Ui {
    fn icon_button(&mut self, glyph: &str, text: &str) -> egui::Response {
        let response = self.button(icon_text(glyph, text));
        // The glyph lives in a private-use area, so leaving it in the accessible
        // name makes a screen reader announce nonsense. Publish the caption.
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, self.is_enabled(), text)
        });
        response
    }
}
/// An EZClient-style toolbar button: a large glyph with a caption underneath,
/// which is how the original's toolbar reads, with a Fluent hover state.
pub fn tool_button(ui: &mut egui::Ui, glyph: &str, caption: &str, tooltip: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(74.0, 58.0), egui::Sense::click());
    let hovered = response.hovered();
    let active = response.is_pointer_button_down_on();
    let painter = ui.painter().clone();

    if hovered || active {
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(6),
            if active {
                CONTROL_PRESSED
            } else {
                CONTROL_HOVER
            },
        );
    }

    let tint = if hovered { TEXT } else { TEXT_DIM };
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 21.0),
        egui::Align2::CENTER_CENTER,
        glyph,
        icon_font(22.0),
        tint,
    );
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 12.0),
        egui::Align2::CENTER_CENTER,
        caption,
        egui::FontId::proportional(11.0),
        tint,
    );

    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    // The caption is painted rather than laid out, so the widget has no text of
    // its own. Publish it as the accessible name: screen readers need it, and so
    // does any accessibility-tree driven automation.
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), caption)
    });
    response.on_hover_text(tooltip)
}
