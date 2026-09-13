//! Fixed navigation sidebar: icons, sections, active highlight, hover,
//! connected-device summary footer.

use crate::state::Page;
use crate::ui::theme::palette;

const NAV_TOP: &[(Page, &str, &str)] = &[
    (Page::Dashboard, "⌂", "Dashboard"),
    (Page::Devices, "◉", "Devices"),
];
const NAV_MANAGE: &[(Page, &str, &str)] = &[
    (Page::Apps, "▦", "Apps"),
    (Page::Processes, "≣", "Processes"),
    (Page::Files, "▤", "Files"),
];
const NAV_DEBUG: &[(Page, &str, &str)] = &[
    (Page::Logcat, "☰", "Logcat"),
    (Page::Apk, "▣", "APK"),
    (Page::Shell, "⌨", "Shell"),
];
const NAV_TOOLS: &[(Page, &str, &str)] = &[
    (Page::Tools, "◈", "Device Tools"),
    (Page::Settings, "⚙", "Settings"),
];

pub fn show(ui: &mut egui::Ui, current: &mut Page) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        // Accent brand mark.
        egui::Frame::new()
            .fill(palette::ACCENT)
            .corner_radius(4.0))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("A")
                        .strong()
                        .color(egui::Color32::WHITE),
                );
            });
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("ADB Manager").strong());
            ui.label(
                egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                    .small()
                    .color(palette::TEXT_DIM),
            );
        });
    });
    ui.add_space(6.0);
    ui.separator();

    for (page, icon, label) in NAV_TOP {
        nav_item(ui, current, *page, icon, label);
    }

    section(ui, "MANAGEMENT");
    for (page, icon, label) in NAV_MANAGE {
        nav_item(ui, current, *page, icon, label);
    }

    section(ui, "DEBUG");
    for (page, icon, label) in NAV_DEBUG {
        nav_item(ui, current, *page, icon, label);
    }

    section(ui, "TOOLS");
    for (page, icon, label) in NAV_TOOLS {
        nav_item(ui, current, *page, icon, label);
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(title)
            .small()
            .strong()
            .color(palette::TEXT_FAINT),
    );
    ui.add_space(2.0);
}

fn nav_item(ui: &mut egui::Ui, current: &mut Page, page: Page, icon: &str, label: &str) {
    let selected = *current == page;
    // Hover fill comes from the theme (`widgets.hovered`); the accent fill
    // marks the active page.
    let resp = ui.add_sized(
        [ui.available_width(), 28.0],
        egui::Button::new(
            egui::RichText::new(format!("{icon}  {label}")).color(if selected {
                egui::Color32::WHITE
            } else {
                palette::TEXT_DIM
            }),
        )
        .fill(if selected {
            palette::ACCENT
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::NONE)
        .corner_radius(4.0)),
    );
    if resp.clicked() {
        *current = page;
    }
}

/// Compact connected-device summary pinned to the sidebar bottom.
/// Rendered by `app.rs` in a bottom panel inside the sidebar.
pub fn device_footer(ui: &mut egui::Ui, state: &mut crate::state::AppState) {
    ui.separator();
    ui.add_space(2.0);
    match state.selected_device().cloned() {
        Some(d) => {
            let color = if d.state.is_usable() {
                palette::SUCCESS
            } else {
                palette::WARNING
            };
            ui.horizontal(|ui| {
                ui.colored_label(color, "●");
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(d.display_name()).strong().small());
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {}",
                            d.state.label(),
                            d.transport.label()
                        ))
                        .small()
                        .color(palette::TEXT_DIM),
                    );
                });
            });
        }
        None => {
            ui.horizontal(|ui| {
                ui.colored_label(palette::TEXT_FAINT, "○");
                ui.label(
                    egui::RichText::new("No device")
                        .small()
                        .color(palette::TEXT_DIM),
                );
            });
        }
    }
}
