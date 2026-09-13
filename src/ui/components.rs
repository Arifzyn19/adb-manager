//! Reusable themed widgets: buttons, badges, panels, tables, states.
//!
//! Pages compose from these — no page hand-rolls widget styling. All visuals
//! derive from `theme::palette` / `theme::metrics`.

use crate::ui::theme::{metrics, palette};
use egui::{Color32, Response, RichText, Ui};

// --- Text ------------------------------------------------------------------

/// Page title + one-line dim description.
pub fn page_header(ui: &mut Ui, title: &str, description: &str) {
    ui.horizontal(|ui| {
        ui.heading(title);
    });
    if !description.is_empty() {
        ui.label(RichText::new(description).color(palette::TEXT_DIM));
    }
    ui.add_space(metrics::GAP_TIGHT);
}

/// Small caps-ish section label.
pub fn section_title(ui: &mut Ui, title: &str) {
    ui.add_space(metrics::GAP_TIGHT);
    ui.label(RichText::new(title).strong().color(palette::TEXT_DIM));
    ui.add_space(2.0);
}

// --- Panels ------------------------------------------------------------------

/// Bordered content panel (cards done sparingly, per design).
pub fn panel<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .fill(palette::PANEL)
        .stroke(egui::Stroke::new(metrics::STROKE, palette::BORDER))
        .corner_radius(metrics::RADIUS_PANEL.into())
        .inner_margin(metrics::PAD_PANEL.into())
        .show(ui, add_contents)
}

/// Sunken panel for terminals / previews (darker than `panel`).
pub fn sunken_panel<R>(
    ui: &mut Ui,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .fill(palette::BG_SUNKEN)
        .stroke(egui::Stroke::new(metrics::STROKE, palette::BORDER))
        .corner_radius(metrics::RADIUS_PANEL.into())
        .inner_margin(metrics::PAD_PANEL.into())
        .show(ui, add_contents)
}

/// Compact stat block: dim label, strong value, dim sub-line.
pub fn stat_block(ui: &mut Ui, label: &str, value: &str, sub: &str) {
    ui.vertical(|ui| {
        ui.set_min_width(120.0);
        ui.label(
            RichText::new(label.to_uppercase())
                .small()
                .color(palette::TEXT_DIM),
        );
        ui.label(RichText::new(value).strong().size(15.0));
        if !sub.is_empty() {
            ui.label(RichText::new(sub).small().color(palette::TEXT_DIM));
        }
    });
}

// --- Buttons -----------------------------------------------------------------

/// Primary action: accent fill, bright text, lighter hover.
pub fn primary_button(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(Color32::WHITE))
            .fill(palette::ACCENT)
            .corner_radius(metrics::RADIUS_WIDGET.into()),
    )
}

/// Secondary action: panel fill, hairline border, brighter border on hover.
pub fn secondary_button(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        egui::Button::new(label)
            .fill(palette::PANEL)
            .stroke(egui::Stroke::new(metrics::STROKE, palette::BORDER))
            .corner_radius(metrics::RADIUS_WIDGET.into()),
    )
}

/// Destructive action: transparent fill, red text/border, red tint on hover.
pub fn danger_button(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(palette::ERROR))
            .fill(Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(metrics::STROKE, palette::ERROR))
            .corner_radius(metrics::RADIUS_WIDGET.into()),
    )
}

/// Small square icon button with tooltip.
pub fn icon_button(ui: &mut Ui, glyph: &str, tooltip: &str) -> Response {
    ui.add_sized(
        [26.0, 22.0],
        egui::Button::new(RichText::new(glyph).color(palette::TEXT_DIM)),
    )
    .on_hover_text(tooltip)
}

/// Disabled-aware wrapper: renders the button dimmed when `enabled` is false.
pub fn enabled_button(ui: &mut Ui, enabled: bool, label: &str) -> Response {
    ui.add_enabled(enabled, egui::Button::new(label))
}

// --- Badges ------------------------------------------------------------------

/// Colored dot + label (status is never color-alone).
pub fn status_badge(ui: &mut Ui, label: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.colored_label(color, "●");
        ui.label(RichText::new(label).color(color).strong());
    });
}

/// Pill badge with tinted background.
pub fn pill(ui: &mut Ui, label: &str, color: Color32, tint: Color32) {
    egui::Frame::new()
        .fill(tint)
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(9.0.into())
        .inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 1,
            bottom: 1,
        })
        .show(ui, |ui| {
            ui.label(RichText::new(label).small().color(color).strong());
        });
}

// --- Inputs ------------------------------------------------------------------

/// Search field with hint text. Returns the edit response (for Enter handling).
pub fn search_field(ui: &mut Ui, text: &mut String, hint: &str) -> Response {
    ui.horizontal(|ui| {
        ui.label(RichText::new("⌕").color(palette::TEXT_FAINT));
        ui.add(
            egui::TextEdit::singleline(text)
                .hint_text(hint)
                .desired_width(f32::INFINITY),
        )
    })
    .inner
}

/// Labeled single-line input row. Returns the edit response.
pub fn field_row(ui: &mut Ui, label: &str, text: &mut String, hint: &str) -> Response {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(palette::TEXT_DIM));
        ui.add(
            egui::TextEdit::singleline(text)
                .hint_text(hint)
                .desired_width(220.0),
        )
    })
    .inner
}

/// Segmented filter tabs. Returns true when the selection changed.
pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, options: &[(T, &str)], current: &mut T) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (value, label) in options {
            let active = *current == **value;
            let resp = ui.add(
                egui::Button::new(RichText::new(*label).color(if active {
                    Color32::WHITE
                } else {
                    palette::TEXT_DIM
                }))
                .fill(if active {
                    palette::ACCENT
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(egui::Stroke::new(
                    metrics::STROKE,
                    if active {
                        palette::ACCENT
                    } else {
                        palette::BORDER
                    },
                ))
                .corner_radius(metrics::RADIUS_WIDGET.into()),
            );
            if resp.clicked() {
                *current = **value;
                changed = true;
            }
        }
    });
    changed
}

// --- Tables ------------------------------------------------------------------

/// Dim small-caps table header row. Callers render body rows with matching
/// fixed widths + `ui.separator()` underneath.
pub fn table_header(ui: &mut Ui, cols: &[(&str, f32)]) {
    ui.horizontal(|ui| {
        for (name, width) in cols {
            ui.add_sized(
                [*width, 18.0],
                egui::Label::new(
                    RichText::new(name.to_uppercase())
                        .small()
                        .color(palette::TEXT_DIM)
                        .strong(),
                ),
            );
        }
    });
    ui.separator();
}

// --- States ------------------------------------------------------------------

/// Polished empty state: glyph, title, body, action button. Returns true
/// when the action button is clicked.
pub fn empty_state(ui: &mut Ui, icon: &str, title: &str, body: &str, action: &str) -> bool {
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        ui.label(RichText::new(icon).size(32.0).color(palette::TEXT_FAINT));
        ui.add_space(4.0);
        ui.label(RichText::new(title).strong().size(15.0));
        ui.add_space(2.0);
        ui.label(RichText::new(body).color(palette::TEXT_DIM));
        ui.add_space(10.0);
        if primary_button(ui, action).clicked() {
            clicked = true;
        }
        ui.add_space(24.0);
    });
    clicked
}

/// No-device empty state shared by every device-bound page.
pub fn no_device_state(ui: &mut Ui, state: &mut crate::state::AppState) {
    if empty_state(
        ui,
        "◉",
        "No device connected",
        "Connect an Android device using USB or Wireless ADB.",
        "Connect Device",
    ) {
        state.show_connect_dialog = true;
    }
}

/// Loading state: spinner + what + current operation. Optional cancel.
pub fn loading_state(ui: &mut Ui, title: &str, detail: &str, cancel_label: Option<&str>) -> bool {
    let mut cancelled = false;
    ui.horizontal(|ui| {
        ui.spinner();
        ui.vertical(|ui| {
            ui.label(RichText::new(title).strong());
            ui.label(RichText::new(detail).color(palette::TEXT_DIM).small());
        });
        if let Some(label) = cancel_label {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secondary_button(ui, label).clicked() {
                    cancelled = true;
                }
            });
        }
    });
    cancelled
}

/// Error panel: human message + expandable technical details.
pub fn error_panel(ui: &mut Ui, message: &str, details: Option<&str>) {
    egui::Frame::new()
        .fill(palette::ERROR_TINT)
        .stroke(egui::Stroke::new(metrics::STROKE, palette::ERROR))
        .corner_radius(metrics::RADIUS_PANEL.into())
        .inner_margin(metrics::PAD_PANEL.into())
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(palette::ERROR, "✕");
                ui.label(RichText::new(message).strong());
            });
            if let Some(d) = details {
                ui.collapsing("Technical details", |ui| {
                    ui.monospace(d);
                });
            }
        });
}

/// Warning notice line.
pub fn warning_line(ui: &mut Ui, message: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(palette::WARNING, "⚠");
        ui.label(RichText::new(message).color(palette::WARNING));
    });
}

/// Generic modal confirm. Returns `Some(true/false)` on click, else `None`.
/// The caller owns the open flag (typically an `Option<...>` in view state).
pub fn confirm_modal(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    lines: &[(&str, bool)],
    confirm_label: &str,
    danger: bool,
) -> Option<bool> {
    let mut result: Option<bool> = None;
    egui::Window::new(title)
        .id(egui::Id::new(id))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            for (text, emph) in lines {
                if *emph {
                    ui.label(RichText::new(*text).strong());
                } else {
                    ui.label(RichText::new(*text).color(palette::TEXT_DIM));
                }
            }
            ui.add_space(metrics::GAP_TIGHT);
            ui.horizontal(|ui| {
                if secondary_button(ui, "Cancel").clicked() {
                    result = Some(false);
                }
                let resp = if danger {
                    danger_button(ui, confirm_label)
                } else {
                    primary_button(ui, confirm_label)
                };
                if resp.clicked() {
                    result = Some(true);
                }
            });
        });
    result
}

// --- Key/value -----------------------------------------------------------------

/// Striped two-column grid for detail views.
pub fn kv_grid(ui: &mut Ui, id: &str, rows: &[(&str, &str)]) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([14.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for (k, v) in rows {
                ui.label(RichText::new(*k).color(palette::TEXT_DIM));
                ui.monospace(*v);
                ui.end_row();
            }
        });
}

/// Single key/value line for compact cards.
pub fn kv_line(ui: &mut Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).color(palette::TEXT_DIM));
        ui.monospace(value);
    });
}
