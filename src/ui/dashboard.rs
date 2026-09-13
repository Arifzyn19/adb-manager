//! Dashboard page: selected-device overview cards (Phase 1: identity only).

use crate::state::{AppState, Page};
use crate::ui::theme::StatusColors;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Dashboard");
    ui.add_space(6.0);

    let Some(device) = state.selected_device().cloned() else {
        empty_state(ui, state);
        return;
    };

    ui.horizontal_wrapped(|ui| {
        card(
            ui,
            "DEVICE",
            &device.display_name(),
            &format!("{}\n{}", device.state.label(), device.transport.label()),
        );
        card(ui, "SERIAL", &device.serial, &detail_lines(&device));
        match state.info.get(&device.serial) {
            Some(info) => card(ui, "SYSTEM", &info.system_summary(), &system_lines(info)),
            None if device.state.is_usable() => card(
                ui,
                "SYSTEM",
                "Loading…",
                "Reading Android version and hardware details from the device.",
            ),
            None => card(
                ui,
                "SYSTEM",
                "Unavailable",
                "Details cannot be read while the device is not connected.",
            ),
        }
        card(
            ui,
            "MEMORY / STORAGE / BATTERY",
            "Live stats in Phase 10",
            "Background workers for battery, memory and storage land with Device Tools.",
        );
    });

    ui.add_space(10.0);
    ui.strong("Quick actions");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Apps").clicked() {
            state.page = Page::Apps;
        }
        if ui.button("Logcat").clicked() {
            state.page = Page::Logcat;
        }
        if ui.button("Files").clicked() {
            state.page = Page::Files;
        }
        if ui.button("Shell").clicked() {
            state.page = Page::Shell;
        }
        if ui.button("Devices").clicked() {
            state.page = Page::Devices;
        }
    });

    if !device.state.is_usable() {
        ui.add_space(8.0);
        ui.colored_label(
            StatusColors::warning(),
            format!(
                "Note: this device is '{}'. {}",
                device.state.label(),
                crate::adb::AdbError::DeviceUnauthorized {
                    serial: device.serial.clone()
                }
                .guidance()
            ),
        );
    }
}

fn detail_lines(d: &crate::adb::Device) -> String {
    let mut lines = Vec::new();
    if let Some(p) = &d.product {
        lines.push(format!("product: {p}"));
    }
    if let Some(m) = &d.model {
        lines.push(format!("model: {m}"));
    }
    if let Some(t) = &d.transport_id {
        lines.push(format!("transport-id: {t}"));
    }
    if lines.is_empty() {
        "No extended details reported by ADB.".to_string()
    } else {
        lines.join("\n")
    }
}

fn system_lines(info: &crate::device::DeviceInfo) -> String {
    let manufacturer = info.manufacturer.as_deref().unwrap_or("—");
    let model = info.model.as_deref().unwrap_or("—");
    let resolution = info.screen_resolution.as_deref().unwrap_or("—");
    format!("{manufacturer} {model}\nDisplay: {resolution}")
}

fn card(ui: &mut egui::Ui, title: &str, headline: &str, body: &str) {
    egui::Frame::group(ui.style())
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_size(egui::vec2(220.0, 110.0));
            ui.strong(title);
            ui.separator();
            ui.label(egui::RichText::new(headline).strong());
            ui.label(body);
        });
}

fn empty_state(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label("No Android device connected.");
    ui.label("Connect a device using USB or Wireless ADB.");
    ui.add_space(6.0);
    if ui.button("Connect Device").clicked() {
        state.show_connect_dialog = true;
    }
}
