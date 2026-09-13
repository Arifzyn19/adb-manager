//! Device Tools page: SCREEN / SYSTEM / INFO / ADB sections with
//! progress, confirms and live snapshots.

use crate::state::{AppState, RecPhase};
use crate::tools::RebootMode;
use crate::ui::components::{self, page_header};
use crate::ui::theme::palette;

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

    page_header(
        ui,
        "Device Tools",
        "Capture, inspect and reboot the selected device.",
    );

    let Some(device) = state.selected_device().cloned() else {
        components::no_device_state(ui, state);
        return actions;
    };
    if !device.state.is_usable() {
        ui.label(
            egui::RichText::new(format!(
                "Device Tools unavailable while the device is '{}'.",
                device.state.label()
            ))
            .color(palette::TEXT_DIM),
        );
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
    components::section_title(ui, "SCREEN");
    components::panel(ui, |ui| {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!state.tools.shot_busy, egui::Button::new("Take Screenshot"))
                .clicked()
            {
                actions.op = Some(ToolOp::Screenshot);
            }
            if state.tools.shot_busy {
                ui.spinner();
                ui.label(
                    egui::RichText::new("Capturing…")
                        .small()
                        .color(palette::TEXT_DIM),
                );
            }
        });
        if let Some(png) = state.tools.shot_png.clone() {
            ui.add(egui::Image::from_bytes("bytes://screenshot.png", png).max_height(320.0));
            ui.horizontal(|ui| {
                if let Some(local) = state.tools.shot_local.clone() {
                    ui.monospace(&local);
                }
                if components::secondary_button(ui, "Save as…").clicked() {
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
            ui.label(egui::RichText::new("Record").color(palette::TEXT_DIM));
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
                    if components::primary_button(ui, "Start Recording").clicked() {
                        if let Some(dest) = rfd::FileDialog::new()
                            .add_filter("MP4 video", &["mp4"])
                            .set_file_name("recording.mp4")
                            .set_title("Save recording as")
                            .save_file()
                        {
                            actions.op = Some(ToolOp::StartRec {
                                secs: state.tools.rec_limit.clamp(1, 180),
                                local: dest.display().to_string(),
                            });
                        }
                    }
                });
                if let Some(local) = state.tools.rec_local.clone() {
                    ui.horizontal(|ui| {
                        ui.colored_label(palette::SUCCESS, "✓");
                        ui.label("Last recording");
                        ui.monospace(&local);
                    });
                }
                if let Some(err) = state.tools.rec_error.clone() {
                    components::error_panel(ui, &err, None);
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
                    if components::danger_button(ui, "Stop").clicked() {
                        actions.op = Some(ToolOp::StopRec);
                    }
                });
            }
        }
        ui.label(
            egui::RichText::new("Device limits: 180 s max, no audio. Pull happens automatically when the recording ends.")
                .small()
                .color(palette::TEXT_FAINT),
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
    components::section_title(ui, "SYSTEM");
    components::panel(ui, |ui| {
        let busy = state.tools.busy.is_some();
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!busy, |ui| {
                for mode in [
                    RebootMode::System,
                    RebootMode::Recovery,
                    RebootMode::Bootloader,
                ] {
                    if components::danger_button(ui, mode.label()).clicked() {
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
        let target = state.selected_serial.clone().unwrap_or_default();
        let line = format!("{} {}?", mode.label(), target);
        match components::confirm_modal(
            ctx,
            "tools-reboot",
            mode.label(),
            &[
                (&line, true),
                (
                    "The device will disconnect. Reconnect it afterwards.",
                    false,
                ),
            ],
            mode.label(),
            true,
        ) {
            Some(true) => {
                state.tools.confirm_reboot = None;
                actions.op = Some(ToolOp::Reboot(mode));
            }
            Some(false) => {
                state.tools.confirm_reboot = None;
            }
            None => {}
        }
    }
}

// --- INFO ------------------------------------------------------------------

fn show_info(ui: &mut egui::Ui, state: &mut AppState, actions: &mut ToolsActions) {
    ui.horizontal(|ui| {
        components::section_title(ui, "INFO");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::secondary_button(ui, "Refresh").clicked() {
                actions.op = Some(ToolOp::RefreshInfo);
            }
        });
    });
    if state.tools.info_loading {
        components::loading_state(
            ui,
            "Reading device info",
            "Battery · memory · storage · properties…",
            None,
        );
    }
    if let Some(err) = state.tools.info_error.clone() {
        components::error_panel(ui, &err, None);
    }

    components::panel(ui, |ui| {
        components::section_title(ui, "BATTERY");
        match state.tools.battery.clone() {
            Some(b) => {
                if let Some(pct) = b.level_pct {
                    ui.add(egui::ProgressBar::new(pct as f32 / 100.0).show_percentage());
                }
                components::kv_grid(
                    ui,
                    "tool-battery",
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
                ui.label(
                    egui::RichText::new("No battery data yet — Refresh.").color(palette::TEXT_DIM),
                );
            }
        }
    });

    components::panel(ui, |ui| {
        components::section_title(ui, "MEMORY");
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
                ui.label(
                    egui::RichText::new(
                        "From /proc/meminfo (MemAvailable); approximate by nature.",
                    )
                    .small()
                    .color(palette::TEXT_FAINT),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new("No memory data yet — Refresh.").color(palette::TEXT_DIM),
                );
            }
        }
    });

    components::panel(ui, |ui| {
        components::section_title(ui, "STORAGE");
        if state.tools.storage.is_empty() {
            ui.label(
                egui::RichText::new("No storage data yet — Refresh.").color(palette::TEXT_DIM),
            );
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
        ui.label(
            egui::RichText::new("Filesystem-level figures from df — not exact app quotas.")
                .small()
                .color(palette::TEXT_FAINT),
        );
    });

    components::panel(ui, |ui| {
        ui.horizontal(|ui| {
            components::section_title(ui, &format!("PROPERTIES ({})", state.tools.props.len()));
        });
        components::search_field(ui, &mut state.tools.props_search, "Search keys or values…");
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
                        ui.label(egui::RichText::new("=").color(palette::TEXT_FAINT));
                        ui.monospace(&v);
                    });
                }
            });
    });
}

// --- ADB -------------------------------------------------------------------

fn show_adb(ui: &mut egui::Ui, state: &mut AppState, actions: &mut ToolsActions) {
    components::section_title(ui, "ADB");
    components::panel(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            if state.tools.busy.is_some() {
                ui.disable();
            }
            if components::secondary_button(ui, "Restart ADB server").clicked() {
                actions.op = Some(ToolOp::RestartAdb);
            }
            if components::secondary_button(ui, "Clear Logcat").clicked() {
                actions.op = Some(ToolOp::ClearLogcat);
            }
        });
        ui.label(
            egui::RichText::new(
                "Restarting ADB drops all connections briefly; devices usually reappear on their own.",
            )
            .small()
            .color(palette::TEXT_FAINT),
        );
    });
}
