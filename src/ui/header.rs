//! Global header: app title + device selector + settings shortcut.

use crate::adb::Device;
use crate::state::{AppState, Page};

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    egui::TopBottomPanel::top("header").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("ADB Manager");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Settings").clicked() {
                    state.page = Page::Settings;
                }
                ui.separator();
                device_selector(ui, state);
            });
        });
    });
}

fn device_selector(ui: &mut egui::Ui, state: &mut AppState) {
    let current_label = state
        .selected_device()
        .map(|d| format!("{} {}", status_dot(d), d.display_name()))
        .unwrap_or_else(|| "No device".to_string());

    egui::ComboBox::from_id_salt("device-selector")
        .selected_text(current_label)
        .width(260.0)
        .show_ui(ui, |ui| {
            if state.devices.is_empty() {
                ui.label("No devices found");
            }
            for d in state.devices.clone() {
                let label = format!("{} {}  [{}]", status_dot(&d), d.display_name(), d.serial);
                if ui
                    .selectable_value(&mut state.selected_serial, Some(d.serial.clone()), label)
                    .clicked()
                {}
            }
            ui.separator();
            if ui.button("+ Connect Device").clicked() {
                state.show_connect_dialog = true;
                ui.close();
            }
        });
}

fn status_dot(d: &Device) -> &'static str {
    if d.state.is_usable() {
        "●"
    } else {
        "○"
    }
}
