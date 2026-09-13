//! Device Tools page (Phase 10).
//!
//! SCREEN (screenshot + recording), SYSTEM (reboot), INFO (battery / memory /
//! storage / properties) and ADB (server restart, clear logcat). Heavy work
//! runs on worker threads; this module renders state and returns actions.

use crate::state::{AppState, RecPhase};
use crate::tools::RebootMode;
use crate::ui::theme::StatusColors;

/// One user intent; executed by `app.rs` on worker threads.
pub enum ToolOp {
    RefreshInfo,
    Screenshot,
    SaveShot(String),
    StartRec { secs: u32, local: String },
    StopRec,
    Reboot(RebootMode),
    RestartAdb,
    ClearLogcat,
}

#[derive(Default)]
pub struct ToolsActions {
    pub op: Option<ToolOp>,
}

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut AppState) -> ToolsActions {
    let mut actions = ToolsActions::default();

    ui.heading("Device Tools");

    let Some(device) = state.selected_device().cloned() else {
        ui.add_space(6.0);
        ui.label("No Android device connected.");
        ui.label("Connect a device using USB or Wireless ADB.");
        if ui.button("Connect Device").clicked() {
            state.show_connect_dialog = true;
        }
        return actions;
    };
    if !device.state.is_usable() {
        ui.label(format!(
            "Device Tools unavailable while the device is '{}'.",
            device.state.label()
        ));
        return actions;
    }

    // Keep the recording timer ticking even with auto-refresh off.
    if state.tools.rec_phase == RecPhase::Recording {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }

    show_screen(ui, state, &mut actions);
    ui.add_space(8.0);
    show_system(ctx, ui, state, &mut actions);
    ui.add_space(8.0);
    show_info(ui, state, &mut actions);
    ui.add_space(8.0);
    show_adb(ui, state, &mut actions);

    actions
}

// --- SCREEN ----------------------------------------------------------------

fn show_screen(ui: &mut egui::Ui, state: &mut AppState, actions: &mut ToolsActions) {
    ui.strong("SCREEN");
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!state.tools.shot_busy, egui::Button::new("Take Screenshot"))
                .clicked()
            {
                actions.op = Some(ToolOp::Screenshot);
            }
            if state.tools.shot_busy {
                ui.spinner();
                ui.label("Capturing…");
            }
        });
        if let Some(png) = state.tools.shot_png.clone() {
            ui.add(
                egui::Image::from_bytes("bytes://screenshot.png", png).max_height(320.0),
            );
            ui.horizontal(|ui| {
                if let Some(local) = state.tools.shot_local.clone() {
                    ui.monospace(&local);
                }
                if ui.button("Save as…").clicked() {
                    if let Some(dest) = rfd::FileDialog::new()
                        .add_filter("PNG image", &["png"])
                        .set_file_name("screenshot.png")
                        .set_title("Save screenshot")
                        .save_file()
                    {
                        actions.op = Some(ToolOp::SaveShot(dest.display().to_string()));
                    }
                }
            });
        }

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label("Record");
            for secs in [15u32, 30, 60, 120, 180] {
                if ui
                    .selectable_label(state.tools.rec_limit == secs, format!("{secs}s"))
                    .clicked()
                {
                    state.tools.rec_limit = secs;
                }
            }
        });
        match state.tools.rec_phase {
            RecPhase::Idle => {
                ui.horizontal(|ui| {
                    if ui.button("Start Recording").clicked() {
                        if let Some(dest) = rfd::FileDialog::new()
                            .add_filter("MP4 video", &["mp4"])
                            .set_file_name("recording.mp4")
                            .set_title("Save recording as")
                            .save_file()
                        {
                            actions.op = Some(ToolOp::StartRec {
                                secs: state.tools.rec_limit.max(1).min(180),
                                local: dest.display().to_string(),
                            });
                        }
                    }
                });
                if let Some(local) = state.tools.rec_local.clone() {
                    ui.horizontal(|ui| {
                        ui.colored_label(StatusColors::connected(), "✓ Last recording");
                        ui.monospace(&local);
                    });
                }
                if let Some(err) = state.tools.rec_error.clone() {
                    ui.colored_label(StatusColors::error(), format!("✕ {err}"));
                }
            }
            RecPhase::Recording => {
                let elapsed = state
                    .tools
                    .rec_started
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.monospace(format!(
                        "Recording… {:02}:{:02} / {}s",
                        elapsed / 60,
                        elapsed % 60,
                        state.tools.rec_limit
                    ));
                    if ui.button("Stop").clicked() {
                        actions.op = Some(ToolOp::StopRec);
                    }
                });
            }
        }
        ui.colored_label(
            StatusColors::muted(),
            "Device limits: 180 s max, no audio. Pull happens automatically when the recording ends.",
        );
    });
}

// --- SYSTEM ----------------------------------------------------------------

fn show_system(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut ToolsActions,
) {
    ui.strong("SYSTEM");
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let busy = state.tools.busy.is_some();
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!busy, |ui| {
                for mode in [
                    RebootMode::System,
                    RebootMode::Recovery,
                    RebootMode::Bootloader,
                ] {
                    if ui.button(mode.label()).clicked() {
                        // Reboots always confirm — the device goes away.
                        state.tools.confirm_reboot = Some(mode);
                    }
                }
            });
            if let Some(b) = state.tools.busy.clone() {
                ui.spinner();
                ui.label(format!("Working… ({b})"));
            }
        });
    });

    if let Some(mode) = state.tools.confirm_reboot {
        egui::Window::new(mode.label())
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "{} {}?",
                    mode.label(),
                    state.selected_serial.clone().unwrap_or_default()
                ));
                ui.colored_label(
                    StatusColors::warning(),
                    "The device will disconnect. Reconnect it afterwards.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        state.tools.confirm_reboot = None;
                    }
                    if ui.button(mode.label()).clicked() {
                        state.tools.confirm_reboot = None;
                        actions.op = Some(ToolOp::Reboot(mode));
                    }
                });
            });
    }
}

// --- INFO ------------------------------------------------------------------

fn show_info(ui: &mut egui::Ui, state: &mut AppState, actions: &mut ToolsActions) {
    ui.horizontal(|ui| {
        ui.strong("INFO");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Refresh").clicked() {
                actions.op = Some(ToolOp::RefreshInfo);
            }
        });
    });
    if state.tools.info_loading {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Reading battery / memory / storage / properties…");
        });
    }
    if let Some(err) = state.tools.info_error.clone() {
        ui.colored_label(StatusColors::error(), format!("✕ {err}"));
    }

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Battery");
        match state.tools.battery.clone() {
            Some(b) => {
                if let Some(pct) = b.level_pct {
                    ui.add(egui::ProgressBar::new(pct as f32 / 100.0).show_percentage());
                }
                info_row(
                    ui,
                    &[
                        ("Status", b.status.label()),
                        ("Health", b.health.label()),
                        (
                            "Temperature",
                            &b.temp_c
                                .map(|t| format!("{t:.1}°C"))
                                .unwrap_or_else(|| "—".to_string()),
                        ),
                        (
                            "Voltage",
                            &b.voltage_mv
                                .map(|v| format!("{v} mV"))
                                .unwrap_or_else(|| "—".to_string()),
                        ),
                        ("Technology", b.technology.as_deref().unwrap_or("—")),
                        (
                            "Powered",
                            &if b.powered.is_empty() {
                                "—".to_string()
                            } else {
                                b.powered.join(" + ")
                            },
                        ),
                    ],
                );
            }
            None => {
                ui.colored_label(StatusColors::muted(), "No battery data yet — Refresh.");
            }
        }
    });

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Memory");
        match state.tools.memory.clone() {
            Some(m) => {
                if let Some(pct) = m.used_pct() {
                    ui.add(egui::ProgressBar::new(pct as f32 / 100.0).show_percentage());
                }
                ui.monospace(format!(
                    "{} used / {} total",
                    crate::apk::inspector::human_size(m.used_kb() * 1024),
                    crate::apk::inspector::human_size(m.total_kb * 1024),
                ));
                ui.colored_label(
                    StatusColors::muted(),
                    "From /proc/meminfo (MemAvailable); approximate by nature.",
                );
            }
            None => {
                ui.colored_label(StatusColors::muted(), "No memory data yet — Refresh.");
            }
        }
    });

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Storage");
        if state.tools.storage.is_empty() {
            ui.colored_label(StatusColors::muted(), "No storage data yet — Refresh.");
        }
        for row in state.tools.storage.clone() {
            ui.horizontal(|ui| {
                ui.monospace(&row.mount);
                ui.monospace(format!(
                    "{} used / {} total",
                    crate::apk::inspector::human_size(row.used_bytes),
                    crate::apk::inspector::human_size(row.total_bytes),
                ));
            });
            if let Some(pct) = row.use_pct_or_compute() {
                ui.add(egui::ProgressBar::new(pct as f32 / 100.0).show_percentage());
            }
        }
        ui.colored_label(
            StatusColors::muted(),
            "Filesystem-level figures from df — not exact app quotas.",
        );
    });

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong(format!("Properties ({})", state.tools.props.len()));
        ui.horizontal(|ui| {
            ui.label("Search");
            ui.text_edit_singleline(&mut state.tools.props_search);
        });
        let query = state.tools.props_search.to_lowercase();
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .show(ui, |ui| {
                for (k, v) in state.tools.props.clone() {
                    if !query.is_empty()
                        && !k.to_lowercase().contains(&query)
                        && !v.to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.monospace(&k);
                        ui.colored_label(StatusColors::muted(), "=");
                        ui.monospace(&v);
                    });
                }
            });
    });
}

fn info_row(ui: &mut egui::Ui, rows: &[(&str, &str)]) {
    egui::Grid::new("tool_info_grid")
        .num_columns(2)
        .show(ui, |ui| {
            for (k, v) in rows {
                ui.label(*k);
                ui.monospace(*v);
                ui.end_row();
            }
        });
}

// --- ADB -------------------------------------------------------------------

fn show_adb(ui: &mut egui::Ui, state: &mut AppState, actions: &mut ToolsActions) {
    ui.strong("ADB");
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            if state.tools.busy.is_some() {
                ui.disable();
            }
            if ui.button("Restart ADB server").clicked() {
                actions.op = Some(ToolOp::RestartAdb);
            }
            if ui.button("Clear Logcat").clicked() {
                actions.op = Some(ToolOp::ClearLogcat);
            }
        });
        ui.colored_label(
            StatusColors::muted(),
            "Restarting ADB drops all connections briefly; devices usually reappear on their own.",
        );
    });
}
