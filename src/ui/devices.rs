//! Devices page: list, selection, per-device details, connect/disconnect,
//! saved wireless devices + auto-reconnect entry points.

use crate::adb::{Device, Transport};
use crate::device::DeviceInfo;
use crate::state::AppState;
use crate::ui::theme::{mono, StatusColors};

#[derive(Default)]
pub struct DevicesActions {
    pub refresh_requested: bool,
    pub disconnect_serial: Option<String>,
    pub connect_serial: Option<String>,
    pub refresh_info_serial: Option<String>,
    pub saved_changed: bool,
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) -> DevicesActions {
    let mut actions = DevicesActions::default();

    ui.horizontal(|ui| {
        ui.heading("Devices");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Connect Device").clicked() {
                state.show_connect_dialog = true;
            }
            if ui.button("Refresh").clicked() {
                actions.refresh_requested = true;
            }
        });
    });
    ui.add_space(4.0);

    if state.devices.is_empty() {
        ui.label("No Android device connected.");
        ui.label("Connect a device using USB or Wireless ADB.");
        ui.add_space(6.0);
        if ui.button("Connect Device").clicked() {
            state.show_connect_dialog = true;
        }
    } else {
        let devices = state.devices.clone();
        for d in &devices {
            device_row(ui, state, d, &mut actions);
        }
    }

    saved_section(ui, state, &mut actions);

    actions
}

fn device_row(ui: &mut egui::Ui, state: &mut AppState, d: &Device, actions: &mut DevicesActions) {
    let selected = state.selected_serial.as_ref() == Some(&d.serial);
    let (dot, color) = if d.state.is_usable() {
        ("●", StatusColors::connected())
    } else {
        ("○", StatusColors::warning())
    };

    egui::Frame::group(ui.style())
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(color, dot);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(d.display_name()).strong());
                    ui.label(mono(d.serial.clone()));
                    ui.label(format!(
                        "{}  •  {}  •  {}",
                        d.state.label(),
                        d.transport.label(),
                        d.model.clone().unwrap_or_else(|| "—".to_string())
                    ));
                    if d.transport == Transport::Usb {
                        if let Some(usb) = &d.usb {
                            ui.label(format!("USB port: {usb}"));
                        } else {
                            ui.label("USB connection");
                        }
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !selected && ui.button("Select").clicked() {
                        state.selected_serial = Some(d.serial.clone());
                    }
                    if selected {
                        ui.colored_label(StatusColors::accent(), "Selected");
                    }
                    // Only wireless links can be dropped from here;
                    // USB devices disconnect physically (unplug).
                    if d.transport == Transport::Wireless
                        && d.state.is_usable()
                        && ui.button("Disconnect").clicked()
                    {
                        actions.disconnect_serial = Some(d.serial.clone());
                    }
                });
            });

            // Per-device details, fetched in the background (see AppState::info).
            ui.collapsing("Details", |ui| {
                details_body(ui, state, d, actions);
            });
        });
    ui.add_space(4.0);
}

fn details_body(ui: &mut egui::Ui, state: &mut AppState, d: &Device, actions: &mut DevicesActions) {
    if !d.state.is_usable() {
        ui.label(format!(
            "Details unavailable while the device is '{}'.",
            d.state.label()
        ));
        if d.state == crate::adb::DeviceState::Unauthorized {
            ui.label("Unlock the phone and accept the USB debugging prompt, then Refresh.");
        }
        return;
    }
    match state.info.get(&d.serial).cloned() {
        None => {
            ui.label("Loading device details…");
        }
        Some(info) => {
            if info.fetch_failed && info.system_summary() == "Unknown" {
                ui.colored_label(StatusColors::warning(), "Could not read device properties.");
                if ui.button("Retry").clicked() {
                    actions.refresh_info_serial = Some(d.serial.clone());
                }
                return;
            }
            detail_grid(ui, &info);
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui.button("Refresh details").clicked() {
                    actions.refresh_info_serial = Some(d.serial.clone());
                }
            });
        }
    }
}

fn detail_grid(ui: &mut egui::Ui, info: &DeviceInfo) {
    egui::Grid::new(format!("devinfo-{}", info.model.as_deref().unwrap_or("?")))
        .num_columns(2)
        .spacing([12.0, 3.0])
        .striped(true)
        .show(ui, |ui| {
            for label in [
                "Manufacturer",
                "Brand",
                "Model",
                "Device",
                "Product",
                "Android",
                "SDK",
                "ABI",
                "Build",
                "Resolution",
                "Density",
            ] {
                ui.label(label);
                ui.monospace(info.get(label));
                ui.end_row();
            }
            ui.label("Fingerprint");
            ui.monospace(info.get("Fingerprint"));
            ui.end_row();
        });
}

fn saved_section(ui: &mut egui::Ui, state: &mut AppState, actions: &mut DevicesActions) {
    ui.add_space(6.0);
    ui.strong("Saved devices");
    if state.saved.devices.is_empty() {
        ui.label(
            "No saved devices yet. Successfully connected wireless devices are remembered here.",
        );
        return;
    }
    let connected: Vec<String> = state.devices.iter().map(|d| d.serial.clone()).collect();
    let saved = state.saved.devices.clone();
    for s in &saved {
        let online = connected.contains(&s.serial);
        ui.horizontal(|ui| {
            ui.colored_label(
                if online {
                    StatusColors::connected()
                } else {
                    StatusColors::muted()
                },
                if online { "●" } else { "○" },
            );
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(s.nickname.clone().unwrap_or_else(|| s.serial.clone()))
                        .strong(),
                );
                ui.monospace(s.serial.clone());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Forget").clicked() {
                    state.saved.forget(&s.serial);
                    actions.saved_changed = true;
                }
                if !online && ui.button("Reconnect").clicked() {
                    actions.connect_serial = Some(s.serial.clone());
                }
                if online {
                    ui.colored_label(StatusColors::accent(), "Online");
                }
            });
        });
    }
}
