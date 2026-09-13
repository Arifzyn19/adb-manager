//! Slim bottom status bar: connection, device, transport, ADB version.

use crate::state::{AdbStatus, AppState};
use crate::ui::theme::palette;

pub fn show(ctx: &egui::Context, state: &AppState) {
    egui::TopBottomPanel::bottom("status_bar")
        .frame(
            egui::Frame::new()
                .fill(palette::SECONDARY)
                .inner_margin(egui::Margin {
                    left: 12,
                    right: 12,
                    top: 4,
                    bottom: 4,
                }),
        )
        .show(ctx, |ui| {
            ui.separator();
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                match state.adb_status {
                    AdbStatus::Ready => {
                        if let Some(d) = state.selected_device() {
                            let ok = d.state.is_usable();
                            status_segment(
                                ui,
                                if ok {
                                    "● Connected"
                                } else {
                                    "● Device issue"
                                },
                                if ok {
                                    palette::SUCCESS
                                } else {
                                    palette::WARNING
                                },
                            );
                            status_segment(ui, &d.display_name(), palette::TEXT);
                            status_segment(ui, d.transport.label(), palette::TEXT_DIM);
                            status_segment(ui, &d.serial, palette::TEXT_FAINT);
                        } else if state.devices.is_empty() {
                            status_segment(ui, "○ No device connected", palette::TEXT_FAINT);
                        } else {
                            status_segment(ui, "○ No device selected", palette::TEXT_FAINT);
                        }
                        if let Some(v) = &state.adb_version {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.monospace(format!("ADB {v}"));
                                },
                            );
                        }
                    }
                    AdbStatus::Unavailable => {
                        status_segment(ui, "⚠ ADB unavailable", palette::WARNING);
                        ui.label(
                            egui::RichText::new(&state.adb_message)
                                .small()
                                .color(palette::TEXT_DIM),
                        );
                    }
                    AdbStatus::Unknown => {
                        status_segment(ui, "○ Starting", palette::TEXT_FAINT);
                        ui.label(
                            egui::RichText::new(&state.adb_message)
                                .small()
                                .color(palette::TEXT_DIM),
                        );
                    }
                }
            });
        });
}

fn status_segment(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).small().color(color).strong());
}
