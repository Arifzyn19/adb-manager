//! ADB Environment / Settings page (Phase 1: ADB + device prefs).

use crate::adb::{candidate_adb_paths, detect_adb, AdbClient};
use crate::events::{AppEvent, Toast};
use crate::state::AppState;
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

    ui.heading("Settings");
    ui.add_space(6.0);

    // ---- ADB section ----
    ui.strong("ADB Environment");
    ui.add_space(2.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("ADB Status");
            ui.label(match state.adb_status {
                crate::state::AdbStatus::Ready => "✓ Ready",
                crate::state::AdbStatus::Unavailable => "✕ Unavailable",
                crate::state::AdbStatus::Unknown => "… Detecting",
            });
        });
        ui.horizontal(|ui| {
            ui.label("ADB Version");
            ui.monospace(state.adb_version.clone().unwrap_or_else(|| "—".to_string()));
        });
        ui.horizontal(|ui| {
            ui.label("ADB Path");
            ui.monospace(
                state
                    .config
                    .adb_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "not configured".to_string()),
            );
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Detect ADB").clicked() {
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
            if ui.button("Browse…").clicked() {
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
            if ui.button("Test ADB").clicked() {
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
            ui.label("Common locations searched:");
            for p in candidate_adb_paths().iter().take(6) {
                ui.monospace(p.display().to_string());
            }
        }
    });

    ui.add_space(8.0);

    // ---- Devices section ----
    ui.strong("Devices");
    egui::Frame::group(ui.style()).show(ui, |ui| {
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
            ui.label("Refresh interval (seconds)");
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

    ui.add_space(8.0);

    // ---- Behavior section ----
    ui.strong("Behavior");
    egui::Frame::group(ui.style()).show(ui, |ui| {
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

    ui.add_space(8.0);

    // ---- Logcat section ----
    ui.strong("Logcat");
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Buffer size (lines)");
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

    ui.add_space(8.0);

    // ---- APK section ----
    ui.strong("APK");
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label("Inspection is built-in (local ZIP + manifest decode). An external aapt2 is optional and only powers an extra dump helper.");
        ui.horizontal(|ui| {
            ui.label("aapt2 path (optional)");
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
            if ui.button("Browse…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select aapt2 executable")
                    .pick_file()
                {
                    state.config.aapt2_path = Some(path);
                    actions.config_changed = true;
                }
            }
            if ui.button("Clear").clicked() {
                state.config.aapt2_path = None;
                actions.config_changed = true;
            }
        });
    });

    ui.add_space(8.0);

    // ---- Files section ----
    ui.strong("Files");
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Browser root");
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
        ui.colored_label(
            egui::Color32::GRAY,
            "Default Android storage root (used for Home + first open).",
        );
    });

    ui.add_space(8.0);

    // ---- Diagnostics ----
    ui.strong("Diagnostics");
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Log directory");
            ui.monospace(crate::config::AppConfig::log_dir().display().to_string());
        });
        if ui.button("Open log directory").clicked() {
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
        if ui.button("Copy diagnostics").clicked() {
            ui.ctx().copy_text(diagnostics_text(state));
            let _ = events.send(AppEvent::Toast(Toast::success(
                "Diagnostics copied to clipboard",
            )));
        }
    });

    if let Some(err) = state.last_error.clone() {
        ui.add_space(8.0);
        ui.strong(format!("Last error: {}", err.title));
        ui.label(err.message);
        if let Some(details) = err.details {
            ui.collapsing("Details", |ui| {
                ui.monospace(details);
            });
        }
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
