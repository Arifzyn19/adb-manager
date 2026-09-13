//! Devices page: connected list with status badges + details, saved
//! wireless devices with reconnect/forget.

use crate::adb::{Device, Transport};
use crate::state::AppState;
use crate::ui::components::{self, page_header};
use crate::ui::theme::{mono, palette};

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
        page_header(
            ui,
            "Devices",
            "Connected hardware and remembered wireless links.",
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::primary_button(ui, "Connect Device").clicked() {
                state.show_connect_dialog = true;
            }
            if components::secondary_button(ui, "Refresh").clicked() {
                actions.refresh_requested = true;
            }
        });
    });

    if state.devices.is_empty() {
        ui.add_space(4.0);
        if components::empty_state(
            ui,
            "◉",
            "No devices found",
            "Connect an Android device using USB or Wireless ADB, then Refresh.",
            "Connect Device",
        ) {
            state.show_connect_dialog = true;
        }
    } else {
        components::section_title(ui, &format!("CONNECTED ({})", state.devices.len()));
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
    let usable = d.state.is_usable();
    let dot = if usable {
        palette::SUCCESS
    } else {
        palette::WARNING
    };

    components::panel(ui, |ui| {
        ui.horizontal(|ui| {
            ui.colored_label(dot, "●");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(d.display_name()).strong());
                    if selected {
                        components::pill(
                            ui,
                            "SELECTED",
                            palette::ACCENT_BRIGHT,
                            palette::ACCENT_TINT,
                        );
                    }
                    components::pill(
                        ui,
                        d.state.label(),
                        if usable {
                            palette::SUCCESS
                        } else {
                            palette::WARNING
                        },
                        if usable {
                            palette::SUCCESS_TINT
                        } else {
                            palette::WARNING_TINT
                        },
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(mono(d.serial.clone()));
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {}",
                            d.transport.label(),
                            d.model
                                .clone()
                                .unwrap_or_else(|| "unknown model".to_string())
                        ))
                        .color(palette::TEXT_DIM)
                        .small(),
                    );
                });
                if d.transport == Transport::Usb {
                    ui.label(
                        egui::RichText::new(match &d.usb {
                            Some(usb) => format!("USB port {usb}"),
                            None => "USB connection".to_string(),
                        })
                        .small()
                        .color(palette::TEXT_FAINT),
                    );
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !selected && components::secondary_button(ui, "Select").clicked() {
                    state.selected_serial = Some(d.serial.clone());
                }
                // Only wireless links can be dropped from here;
                // USB devices disconnect physically (unplug).
                if d.transport == Transport::Wireless
                    && usable
                    && components::secondary_button(ui, "Disconnect").clicked()
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
        ui.label(
            egui::RichText::new(format!(
                "Details unavailable while the device is '{}'.",
                d.state.label()
            ))
            .color(palette::TEXT_DIM),
        );
        if d.state == crate::adb::DeviceState::Unauthorized {
            components::warning_line(
                ui,
                "Unlock the phone and accept the USB debugging prompt, then Refresh.",
            );
        }
        return;
    }
    match state.info.get(&d.serial).cloned() {
        None => {
            components::loading_state(
                ui,
                "Reading device details",
                "getprop · wm size · wm density",
                None,
            );
        }
        Some(info) => {
            if info.fetch_failed && info.system_summary() == "Unknown" {
                components::error_panel(
                    ui,
                    "Could not read device properties.",
                    Some("getprop failed — the device may have gone away mid-query."),
                );
                if components::secondary_button(ui, "Retry").clicked() {
                    actions.refresh_info_serial = Some(d.serial.clone());
                }
                return;
            }
            components::kv_grid(
                ui,
                &format!("devinfo-{}", info.model.as_deref().unwrap_or("?")),
                &[
                    ("Manufacturer", info.manufacturer.as_deref().unwrap_or("—")),
                    ("Brand", info.brand.as_deref().unwrap_or("—")),
                    ("Model", info.model.as_deref().unwrap_or("—")),
                    ("Device", info.device_name.as_deref().unwrap_or("—")),
                    ("Product", info.product.as_deref().unwrap_or("—")),
                    ("Android", info.android_version.as_deref().unwrap_or("—")),
                    ("API", info.sdk_version.as_deref().unwrap_or("—")),
                    ("ABI", info.architecture.as_deref().unwrap_or("—")),
                    ("Build", info.build_id.as_deref().unwrap_or("—")),
                    ("Display", info.screen_resolution.as_deref().unwrap_or("—")),
                    ("Density", info.density.as_deref().unwrap_or("—")),
                    ("Fingerprint", info.fingerprint.as_deref().unwrap_or("—")),
                ],
            );
            ui.add_space(4.0);
            if components::secondary_button(ui, "Refresh details").clicked() {
                actions.refresh_info_serial = Some(d.serial.clone());
            }
        }
    }
}

fn saved_section(ui: &mut egui::Ui, state: &mut AppState, actions: &mut DevicesActions) {
    components::section_title(ui, "SAVED DEVICES");
    if state.saved.devices.is_empty() {
        ui.label(
            egui::RichText::new(
                "No saved devices yet — connected wireless devices are remembered here.",
            )
            .color(palette::TEXT_DIM),
        );
        return;
    }
    let connected: Vec<String> = state.devices.iter().map(|d| d.serial.clone()).collect();
    let saved = state.saved.devices.clone();
    for s in &saved {
        let online = connected.contains(&s.serial);
        components::panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    if online {
                        palette::SUCCESS
                    } else {
                        palette::TEXT_FAINT
                    },
                    "●",
                );
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(s.nickname.clone().unwrap_or_else(|| s.serial.clone()))
                            .strong(),
                    );
                    ui.monospace(s.serial.clone());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if components::secondary_button(ui, "Forget").clicked() {
                        state.saved.forget(&s.serial);
                        actions.saved_changed = true;
                    }
                    if !online {
                        if components::primary_button(ui, "Reconnect").clicked() {
                            actions.connect_serial = Some(s.serial.clone());
                        }
                    } else {
                        components::pill(ui, "ONLINE", palette::SUCCESS, palette::SUCCESS_TINT);
                    }
                });
            });
        });
        ui.add_space(4.0);
    }
}
