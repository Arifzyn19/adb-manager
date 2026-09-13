//! Dashboard: selected-device overview — identity, specs, live snapshots,
//! quick actions. Stats reuse the Device Tools snapshots when available.

use crate::state::{AppState, Page};
use crate::ui::components::{self, page_header};
use crate::ui::theme::palette;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    page_header(ui, "Dashboard", "Selected device at a glance.");

    let Some(device) = state.selected_device().cloned() else {
        components::no_device_state(ui, state);
        return;
    };

    // --- Device identity -----------------------------------------------------
    components::panel(ui, |ui| {
        ui.horizontal(|ui| {
            let dot = if device.state.is_usable() {
                palette::SUCCESS
            } else {
                palette::WARNING
            };
            ui.colored_label(dot, "●");
            ui.label(
                egui::RichText::new(device.display_name())
                    .strong()
                    .size(16.0),
            );
            ui.label(egui::RichText::new(device.state.label()).color(palette::TEXT_DIM));
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            components::pill(
                ui,
                device.transport.label(),
                palette::ACCENT_BRIGHT,
                palette::ACCENT_TINT,
            );
            ui.monospace(&device.serial);
            if let Some(model) = device.model.as_deref() {
                ui.label(egui::RichText::new(model).color(palette::TEXT_DIM));
            }
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (label, page) in [
                ("Apps", Page::Apps),
                ("Logcat", Page::Logcat),
                ("Files", Page::Files),
                ("Shell", Page::Shell),
                ("Device Tools", Page::Tools),
            ] {
                if components::secondary_button(ui, label).clicked() {
                    state.page = page;
                }
            }
        });
    });

    ui.add_space(8.0);

    // --- Specs + live snapshots ----------------------------------------------
    ui.columns(2, |cols| {
        components::panel(&mut cols[0], |ui| {
            components::section_title(ui, "SYSTEM");
            match state.info.get(&device.serial).cloned() {
                Some(info) => {
                    components::kv_grid(
                        ui,
                        "dash-specs",
                        &[
                            ("Android", info.android_version.as_deref().unwrap_or("—")),
                            ("API", info.sdk_version.as_deref().unwrap_or("—")),
                            ("ABI", info.architecture.as_deref().unwrap_or("—")),
                            ("Model", info.model.as_deref().unwrap_or("—")),
                            ("Display", info.screen_resolution.as_deref().unwrap_or("—")),
                            ("Build", info.build_id.as_deref().unwrap_or("—")),
                        ],
                    );
                }
                None if device.state.is_usable() => {
                    components::loading_state(
                        ui,
                        "Reading device details",
                        "Android version, hardware, display…",
                        None,
                    );
                }
                None => {
                    ui.label(
                        egui::RichText::new("Unavailable while the device is not connected.")
                            .color(palette::TEXT_DIM),
                    );
                }
            }
        });
        components::panel(&mut cols[1], |ui| {
            components::section_title(ui, "LIVE");
            live_stats(ui, state, &device.serial);
        });
    });

    if !device.state.is_usable() {
        ui.add_space(6.0);
        components::warning_line(
            ui,
            &format!(
                "This device is '{}'. {}",
                device.state.label(),
                crate::adb::AdbError::DeviceUnauthorized {
                    serial: device.serial.clone()
                }
                .guidance()
            ),
        );
    }
}

fn live_stats(ui: &mut egui::Ui, state: &mut AppState, serial: &str) {
    let fresh = state.tools.serial.as_deref() == Some(serial);
    let has_any = fresh
        && (state.tools.battery.is_some()
            || state.tools.memory.is_some()
            || !state.tools.storage.is_empty());
    if !has_any {
        ui.label(egui::RichText::new("No live snapshots yet.").color(palette::TEXT_DIM));
        ui.add_space(4.0);
        if components::secondary_button(ui, "Open Device Tools").clicked() {
            state.page = Page::Tools;
        }
        return;
    }

    if let Some(b) = state.tools.battery.clone() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Battery").color(palette::TEXT_DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{}% · {}",
                        b.level_pct
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "—".to_string()),
                        b.status.label()
                    ))
                    .strong(),
                );
            });
        });
        if let Some(pct) = b.level_pct {
            ui.add(egui::ProgressBar::new(pct as f32 / 100.0).show_percentage());
        }
    }
    if let Some(m) = state.tools.memory.clone() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Memory").color(palette::TEXT_DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.monospace(format!(
                    "{} / {}",
                    crate::apk::inspector::human_size(m.used_kb() * 1024),
                    crate::apk::inspector::human_size(m.total_kb * 1024),
                ));
            });
        });
        if let Some(pct) = m.used_pct() {
            ui.add(egui::ProgressBar::new(pct as f32 / 100.0).show_percentage());
        }
    }
    let storage = state.tools.storage.clone();
    if let Some(row) = storage
        .iter()
        .find(|r| r.mount == "/sdcard")
        .or_else(|| storage.first())
    {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Storage").color(palette::TEXT_DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.monospace(format!(
                    "{} / {}",
                    crate::apk::inspector::human_size(row.used_bytes),
                    crate::apk::inspector::human_size(row.total_bytes),
                ));
            });
        });
        if let Some(pct) = row.use_pct_or_compute() {
            ui.add(egui::ProgressBar::new(pct as f32 / 100.0).show_percentage());
        }
    }
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new("Snapshots refresh on the Device Tools page.")
            .small()
            .color(palette::TEXT_FAINT),
    );
}
