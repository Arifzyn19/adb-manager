//! Global bottom status bar.

use crate::state::{AdbStatus, AppState};
use crate::ui::theme::StatusColors;

pub fn show(ctx: &egui::Context, state: &AppState) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| match state.adb_status {
            AdbStatus::Ready => {
                ui.colored_label(StatusColors::connected(), "●");
                if let Some(d) = state.selected_device() {
                    ui.label(format!(
                        "Connected — {}  |  Transport: {}",
                        d.display_name(),
                        d.transport.label()
                    ));
                } else if state.devices.is_empty() {
                    ui.label("○ No device connected");
                } else {
                    ui.label("No device selected");
                }
                if let Some(v) = &state.adb_version {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.monospace(format!("ADB {v}"));
                    });
                }
            }
            AdbStatus::Unavailable => {
                ui.colored_label(StatusColors::warning(), "⚠");
                ui.label(format!("ADB unavailable — {}", state.adb_message));
            }
            AdbStatus::Unknown => {
                ui.colored_label(StatusColors::muted(), "○");
                ui.label(&state.adb_message);
            }
        });
    });
}
