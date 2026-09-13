//! Dark developer-tool theme.

use egui::{Color32, Visuals};

pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.window_corner_radius = 6.0.into();
    visuals.menu_corner_radius = 4.0.into();
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(0x1e, 0x22, 0x2a);
    visuals.panel_fill = Color32::from_rgb(0x16, 0x19, 0x20);
    visuals.extreme_bg_color = Color32::from_rgb(0x0e, 0x11, 0x14);
    visuals.selection.bg_fill = Color32::from_rgb(0x2f, 0x6f, 0xed);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    ctx.set_style(style);
}

/// Semantic status colors (never the *only* status signal — always paired
/// with text/icons).
pub struct StatusColors;

impl StatusColors {
    pub fn connected() -> Color32 {
        Color32::from_rgb(0x4c, 0xaf, 0x50)
    }
    pub fn warning() -> Color32 {
        Color32::from_rgb(0xe6, 0xa2, 0x3c)
    }
    pub fn error() -> Color32 {
        Color32::from_rgb(0xe5, 0x4f, 0x4f)
    }
    pub fn accent() -> Color32 {
        Color32::from_rgb(0x5a, 0xa9, 0xff)
    }
    pub fn muted() -> Color32 {
        Color32::from_rgb(0x8b, 0x94, 0xa3)
    }
}

/// Monospace helper for logcat / shell / stack traces / technical values.
pub fn mono(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into()).monospace()
}
