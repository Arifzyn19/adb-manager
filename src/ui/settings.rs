//! Settings page: ADB environment, device prefs, behavior, per-feature
//! tuning, diagnostics — all in bordered sections.

use crate::adb::{candidate_adb_paths, detect_adb, AdbClient};
use crate::events::{AppEvent, Toast};
use crate::state::AppState;
use crate::ui::components::{self, page_header};
use crate::ui::theme::palette;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

pub struct SettingsActions {
    pub adb_path_changed: Option<PathBuf>,
    pub config_changed: bool,
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState, events: &Sender<AppEvent>) -> SettingsActions {
    let mut actions = SettingsActions {
        adb_path_changed: None,
        config_changed: false,
    };

    page_header(ui, "Settings", "Environment, behavior and tuning.");

    // ---- ADB section ----
    components::section_title(ui, "ADB ENVIRONMENT");
    components::panel(ui, |ui| {
        let (dot, status) = match state.adb_status {
            crate::state::AdbStatus::Ready => (palette::SUCCESS, "Ready"),
            crate::state::AdbStatus::Unavailable => (palette::WARNING, "Unavailable"),
            crate::state::AdbStatus::Unknown => (palette::TEXT_FAINT, "Detecting"),
        };
        ui.horizontal(|ui| {
            ui.colored_label(dot, "●");
            ui.label(egui::RichText::new(format!("ADB Status — {status}")).strong());
        });
        components::kv_grid(
            ui,
            "settings-adb",
            &[
                (
                    "Version",
                    &state.adb_version.clone().unwrap_or_else(|| "—".to_string()),
                ),
                (
                    "Path",
                    &state
                        .config
                        .adb_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "not configured".to_string()),
                ),
            ],
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if components::secondary_button(ui, "Detect ADB").clicked() {
                match detect_adb() {
                    Ok((path, version)) => {
                        state.config.adb_path = Some(path.clone());
                        actions.adb_path_changed = Some(path.clone());
                        actions.config_changed = true;
                        let _ = events.send(AppEvent::Toast(Toast::success(format!(
                            "ADB found: {version}"
                        ))));
                    }
                    Err(e) => {
                        state.set_adb_error(&e);
                        let _ = events.send(AppEvent::Error {
                            title: e.title().to_string(),
                            message: e.guidance().to_string(),
                            details: Some(e.to_string()),
                        });
                    }
                }
            }
            if components::secondary_button(ui, "Browse…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("adb", &["exe", ""])
                    .set_title("Select adb executable")
                    .pick_file()
                {
                    match AdbClient::validate_path(&path) {
                        Ok(version) => {
                            state.config.adb_path = Some(path.clone());
                            actions.adb_path_changed = Some(path);
                            actions.config_changed = true;
                            let _ = events.send(AppEvent::Toast(Toast::success(format!(
                                "ADB validated: {version}"
                            ))));
                        }
                        Err(e) => {
                            state.set_adb_error(&e);
                            let _ = events.send(AppEvent::Error {
                                title: e.title().to_string(),
                                message: e.guidance().to_string(),
                                details: Some(e.to_string()),
                            });
                        }
                    }
                }
            }
            if components::secondary_button(ui, "Test ADB").clicked() {
                match state.config.adb_path.clone() {
                    Some(path) => match AdbClient::validate_path(&path) {
                        Ok(version) => {
                            let _ = events.send(AppEvent::Toast(Toast::success(format!(
                                "ADB works: version {version}"
                            ))));
                        }
                        Err(e) => {
                            state.set_adb_error(&e);
                        }
                    },
                    None => {
                        let _ = events.send(AppEvent::Toast(Toast::warning(
                            "No ADB path configured yet",
                        )));
                    }
                }
            }
        });

        if state.config.adb_path.is_none() {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Common locations searched:")
                    .small()
                    .color(palette::TEXT_DIM),
            );
            for p in candidate_adb_paths().iter().take(6) {
                ui.monospace(p.display().to_string());
            }
        }
    });

    // ---- Devices section ----
    components::section_title(ui, "DEVICES");
    components::panel(ui, |ui| {
        if ui
            .checkbox(&mut state.config.auto_refresh, "Auto refresh device list")
            .changed()
        {
            actions.config_changed = true;
        }
        if ui
            .checkbox(
                &mut state.config.auto_reconnect,
                "Auto reconnect known devices",
            )
            .changed()
        {
            actions.config_changed = true;
        }
        if ui
            .checkbox(&mut state.config.remember_devices, "Remember devices")
            .changed()
        {
            actions.config_changed = true;
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Refresh interval (seconds)").color(palette::TEXT_DIM));
            let mut secs = state.config.refresh_interval_secs as i32;
            if ui
                .add(egui::DragValue::new(&mut secs).range(1..=60).speed(1.0))
                .changed()
            {
                state.config.refresh_interval_secs = secs.clamp(1, 60) as u64;
                actions.config_changed = true;
            }
        });
    });

    // ---- Behavior section ----
    components::section_title(ui, "BEHAVIOR");
    components::panel(ui, |ui| {
        if ui
            .checkbox(
                &mut state.config.confirm_destructive,
                "Confirm destructive actions",
            )
            .changed()
        {
            actions.config_changed = true;
        }
    });

    // ---- Logcat section ----
    components::section_title(ui, "LOGCAT");
    components::panel(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Buffer size (lines)").color(palette::TEXT_DIM));
            let mut size = state.config.log_buffer_size as i32;
            if ui
                .add(
                    egui::DragValue::new(&mut size)
                        .range(1000..=100_000)
                        .speed(100.0),
                )
                .changed()
            {
                state.config.log_buffer_size = size.clamp(1000, 100_000) as usize;
                actions.config_changed = true;
            }
        });
        if ui
            .checkbox(&mut state.config.log_auto_scroll, "Auto-scroll to newest")
            .changed()
        {
            actions.config_changed = true;
        }
        if ui
            .checkbox(&mut state.config.pause_on_crash, "Pause stream on crash")
            .changed()
        {
            actions.config_changed = true;
        }
    });

    // ---- APK section ----
    components::section_title(ui, "APK");
    components::panel(ui, |ui| {
        ui.label(
            egui::RichText::new("Inspection is built-in (local ZIP + manifest decode). An external aapt2 is optional.")
                .small()
                .color(palette::TEXT_DIM),
        );
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("aapt2 path (optional)").color(palette::TEXT_DIM));
            ui.monospace(
                state
                    .config
                    .aapt2_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "not set".to_string()),
            );
        });
        ui.horizontal_wrapped(|ui| {
            if components::secondary_button(ui, "Browse…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select aapt2 executable")
                    .pick_file()
                {
                    state.config.aapt2_path = Some(path);
                    actions.config_changed = true;
                }
            }
            if components::secondary_button(ui, "Clear").clicked() {
                state.config.aapt2_path = None;
                actions.config_changed = true;
            }
        });
    });

    // ---- Files section ----
    components::section_title(ui, "FILES");
    components::panel(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Browser root").color(palette::TEXT_DIM));
            if ui
                .text_edit_singleline(&mut state.config.files_root)
                .changed()
            {
                if state.config.files_root.trim().is_empty() {
                    state.config.files_root = "/sdcard".to_string();
                }
                actions.config_changed = true;
            }
        });
        ui.label(
            egui::RichText::new("Default Android storage root (used for Home + first open).")
                .small()
                .color(palette::TEXT_FAINT),
        );
    });

    // ---- Shortcuts ----
    components::section_title(ui, "SHORTCUTS");
    components::panel(ui, |ui| {
        components::kv_grid(
            ui,
            "settings-shortcuts",
            &[
                ("Ctrl+K", "Command palette"),
                ("Ctrl+Shift+L", "Logcat"),
                ("Ctrl+Shift+A", "Apps"),
                ("Ctrl+Shift+S", "Shell"),
                ("Ctrl+R", "Refresh devices"),
                ("Esc", "Close palette / dialogs"),
            ],
        );
    });

    // ---- Diagnostics ----
    components::section_title(ui, "DIAGNOSTICS");
    components::panel(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Log directory").color(palette::TEXT_DIM));
            ui.monospace(crate::config::AppConfig::log_dir().display().to_string());
        });
        ui.horizontal_wrapped(|ui| {
            if components::secondary_button(ui, "Open log directory").clicked() {
                let dir = crate::config::AppConfig::log_dir();
                let _ = std::fs::create_dir_all(&dir);
                #[cfg(windows)]
                {
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
                #[cfg(not(windows))]
                {
                    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                }
            }
            if components::secondary_button(ui, "Copy diagnostics").clicked() {
                ui.ctx().copy_text(diagnostics_text(state));
                let _ = events.send(AppEvent::Toast(Toast::success(
                    "Diagnostics copied to clipboard",
                )));
            }
        });
    });

    if let Some(err) = state.last_error.clone() {
        ui.add_space(4.0);
        components::error_panel(
            ui,
            &format!("{}: {}", err.title, err.message),
            err.details.as_deref(),
        );
    }

    actions
}

/// One-shot support bundle: versions, ADB state, devices, paths. No device
/// file contents, no credentials — safe to paste into a bug report.
fn diagnostics_text(state: &AppState) -> String {
    let mut out = String::new();
    out.push_str(&format!("ADB Manager {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!(
        "ADB: {} ({})\n",
        state.adb_version.clone().unwrap_or_else(|| "—".to_string()),
        state
            .config
            .adb_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "not configured".to_string()),
    ));
    out.push_str(&format!("Devices: {}\n", state.devices.len()));
    for d in &state.devices {
        out.push_str(&format!(
            "  {} [{}] via {}\n",
            d.serial,
            d.raw_state,
            d.transport.label(),
        ));
    }
    out.push_str(&format!(
        "Config: {}\n",
        crate::config::AppConfig::file_path().display()
    ));
    out.push_str(&format!(
        "Logs: {}\n",
        crate::config::AppConfig::log_dir().display()
    ));
    out
}
