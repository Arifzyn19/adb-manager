//! Centralized visual system: palette, widget states, typography, metrics.
//!
//! Every page builds from here (`theme`) plus `components` — no page invents
//! its own colors, radii or spacing. The backend never touches this module.

use egui::{Color32, Visuals};

/// Exact palette (dark developer tool — no pure-white surfaces anywhere).
pub mod palette {
    use egui::Color32;

    pub const BG: Color32 = Color32::from_rgb(0x0D, 0x11, 0x17);
    pub const BG_SUNKEN: Color32 = Color32::from_rgb(0x09, 0x0C, 0x11);
    pub const SECONDARY: Color32 = Color32::from_rgb(0x11, 0x18, 0x27);
    pub const PANEL: Color32 = Color32::from_rgb(0x16, 0x1B, 0x22);
    pub const PANEL_HOVER: Color32 = Color32::from_rgb(0x1C, 0x23, 0x30);
    pub const BORDER: Color32 = Color32::from_rgb(0x27, 0x31, 0x42);
    pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x37, 0x44, 0x5C);

    pub const TEXT: Color32 = Color32::from_rgb(0xF3, 0xF4, 0xF6);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9C, 0xA3, 0xAF);
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);

    pub const ACCENT: Color32 = Color32::from_rgb(0x3B, 0x82, 0xF6);
    pub const ACCENT_BRIGHT: Color32 = Color32::from_rgb(0x60, 0xA5, 0xFA);
    pub const ACCENT_DEEP: Color32 = Color32::from_rgb(0x1D, 0x4E, 0xD8);
    pub const ACCENT_TINT: Color32 = Color32::from_rgba_unmultiplied(0x3B, 0x82, 0xF6, 26);

    pub const SUCCESS: Color32 = Color32::from_rgb(0x22, 0xC5, 0x5E);
    pub const SUCCESS_TINT: Color32 = Color32::from_rgba_unmultiplied(0x22, 0xC5, 0x5E, 26);
    pub const WARNING: Color32 = Color32::from_rgb(0xF5, 0x9E, 0x0B);
    pub const WARNING_TINT: Color32 = Color32::from_rgba_unmultiplied(0xF5, 0x9E, 0x0B, 26);
    pub const ERROR: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);
    pub const ERROR_TINT: Color32 = Color32::from_rgba_unmultiplied(0xEF, 0x44, 0x44, 28);
    pub const ERROR_DEEP: Color32 = Color32::from_rgb(0x2A, 0x12, 0x15);

    // Log-level semantics (viewer only).
    pub const LOG_VERBOSE: Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);
    pub const LOG_DEBUG: Color32 = Color32::from_rgb(0x58, 0xA6, 0xFF);
    pub const LOG_INFO: Color32 = Color32::from_rgb(0x3F, 0xB9, 0x50);
    pub const LOG_WARN: Color32 = WARNING;
    pub const LOG_ERROR: Color32 = Color32::from_rgb(0xF8, 0x51, 0x49);
}

/// Corner radii, spacing and sizing — single source of truth.
pub mod metrics {
    pub const RADIUS_PANEL: f32 = 6.0;
    pub const RADIUS_WIDGET: f32 = 4.0;
    pub const STROKE: f32 = 1.0;

    pub const PAD_PAGE: f32 = 12.0;
    pub const PAD_PANEL: f32 = 10.0;
    pub const GAP_SECTION: f32 = 10.0;
    pub const GAP_GROUP: f32 = 6.0;
    pub const GAP_TIGHT: f32 = 4.0;

    pub const FONT_BODY: f32 = 13.0;
    pub const FONT_SMALL: f32 = 11.5;
    pub const FONT_HEADING: f32 = 17.0;
    pub const FONT_MONO: f32 = 12.5;
}

pub fn apply_theme(ctx: &egui::Context) {
    use palette::*;
    let mut visuals = Visuals::dark();

    visuals.window_fill = SECONDARY;
    visuals.panel_fill = BG;
    visuals.faint_bg_color = PANEL;
    visuals.extreme_bg_color = BG_SUNKEN;
    visuals.code_bg_color = BG_SUNKEN;

    visuals.window_corner_radius = metrics::RADIUS_PANEL.into();
    visuals.menu_corner_radius = metrics::RADIUS_WIDGET.into();
    // Flat chrome: depth comes from borders, not shadows.
    visuals.window_shadow = egui::Shadow::NONE;
    visuals.popup_shadow = egui::Shadow::NONE;

    visuals.selection = egui::Selection {
        bg_fill: ACCENT,
        stroke: egui::Stroke::new(1.0, TEXT),
    };
    visuals.hyperlink_color = ACCENT_BRIGHT;
    visuals.warn_fg_color = WARNING;
    visuals.error_fg_color = ERROR;
    visuals.disabled_alpha = 0.45;

    // Text cursor matches the accent.
    visuals.text_cursor = egui::TextCursorStyle {
        stroke: egui::Stroke::new(2.0, ACCENT_BRIGHT),
        ..Default::default()
    };

    // Idle buttons/inputs: dark fill, hairline border, bright text.
    visuals.widgets.noninteractive = widget_visuals(PANEL, BORDER, TEXT_DIM);
    visuals.widgets.inactive = widget_visuals(PANEL, BORDER, TEXT);
    visuals.widgets.hovered = widget_visuals(PANEL_HOVER, BORDER_STRONG, TEXT);
    visuals.widgets.active = widget_visuals(ACCENT_TINT, ACCENT, ACCENT_BRIGHT);
    visuals.widgets.open = widget_visuals(PANEL_HOVER, ACCENT, TEXT);

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.indent = 16.0;
    style.spacing.combo_width = 220.0;
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: 8.0,
        ..Default::default()
    };
    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::new(metrics::FONT_HEADING, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::new(metrics::FONT_BODY, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(metrics::FONT_BODY, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            egui::FontId::new(metrics::FONT_SMALL, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(metrics::FONT_MONO, egui::FontFamily::Monospace),
        ),
    ]
    .into();
    ctx.set_style(style);
}

fn widget_visuals(fill: Color32, border: Color32, fg: Color32) -> egui::WidgetVisuals {
    egui::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: egui::Stroke::new(metrics::STROKE, border),
        corner_radius: metrics::RADIUS_WIDGET.into(),
        fg_stroke: egui::Stroke::new(1.0, fg),
        expansion: 0.0,
    }
}

/// Semantic status colors (always paired with text/icons, never alone).
pub struct StatusColors;

impl StatusColors {
    pub fn connected() -> Color32 {
        palette::SUCCESS
    }
    pub fn warning() -> Color32 {
        palette::WARNING
    }
    pub fn error() -> Color32 {
        palette::ERROR
    }
    pub fn accent() -> Color32 {
        palette::ACCENT_BRIGHT
    }
    pub fn muted() -> Color32 {
        palette::TEXT_DIM
    }
}

/// Monospace helper for logcat / shell / stack traces / technical values.
pub fn mono(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into()).monospace()
}
