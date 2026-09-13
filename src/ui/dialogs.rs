//! Connect Device dialog.
//!
//! - USB: guidance + visible devices.
//! - Wireless (Phase 3): QR scan (camera or image file, decoded locally) +
//!   pairing-code form with explicit pairing state.
//! - Manual: `adb connect IP:ADB_PORT` (connection port ≠ pairing port).

use crate::adb::AdbClient;
use crate::events::{AppEvent, Toast};
use crate::pairing::{
    decode_first_qr, decode_wireless_qr, parse_wireless_qr, validate_pair_input, CameraScanner,
    PairingRequest, PairingState,
};
use crate::state::AppState;
use std::sync::mpsc::Sender;

#[derive(Default)]
pub struct ConnectDialogState {
    pub tab: ConnectTab,
    // Manual connect.
    pub ip: String,
    pub port: String,
    pub status: Option<String>,
    pub busy: bool,
    // Wireless pairing form.
    pub pair_ip: String,
    pub pair_port: String,
    pub pair_code: String,
    pub pair_form_error: Option<String>,
    // QR scanning.
    pub camera: Option<CameraScanner>,
    pub qr_texture: Option<egui::TextureHandle>,
    pub qr_status: Option<String>,
    pub qr_frames: u64,
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum ConnectTab {
    #[default]
    Usb,
    Wireless,
    Manual,
}

#[derive(Default)]
pub struct DialogActions {
    /// Set when the user submits a valid pairing form; `app.rs` runs
    /// `adb pair` on a worker thread and reports back via event.
    pub pair_request: Option<PairingRequest>,
}

pub fn show(
    ctx: &egui::Context,
    state: &mut AppState,
    dlg: &mut ConnectDialogState,
    events: &Sender<AppEvent>,
) -> DialogActions {
    let mut actions = DialogActions::default();
    let mut open = state.show_connect_dialog;
    egui::Window::new("Connect Device")
        .open(&mut open)
        .resizable(true)
        .default_width(480.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut dlg.tab, ConnectTab::Usb, "USB");
                ui.selectable_value(&mut dlg.tab, ConnectTab::Wireless, "Wireless");
                ui.selectable_value(&mut dlg.tab, ConnectTab::Manual, "Manual");
            });
            ui.separator();

            match dlg.tab {
                ConnectTab::Usb => usb_tab(ui, state),
                ConnectTab::Wireless => wireless_tab(ctx, ui, state, dlg, events, &mut actions),
                ConnectTab::Manual => manual_tab(ui, state, dlg, events),
            }
        });
    state.show_connect_dialog = open;
    if !open {
        // Release the camera as soon as the dialog closes (§57 cleanup).
        dlg.camera = None;
    }
    actions
}

fn usb_tab(ui: &mut egui::Ui, state: &AppState) {
    ui.label("Connect your phone via USB with USB debugging enabled.");
    ui.label("Accept the on-device authorization prompt if asked.");
    ui.add_space(4.0);
    ui.label(format!(
        "Devices visible right now: {}",
        state.devices.len()
    ));
    for d in &state.devices {
        ui.monospace(format!("{}  ({})", d.serial, d.state.label()));
    }
    if state.devices.iter().any(|d| !d.state.is_usable()) {
        ui.colored_label(
            egui::Color32::YELLOW,
            "An unauthorized/offline device is visible — check the phone screen.",
        );
    }
}

fn wireless_tab(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    state: &mut AppState,
    dlg: &mut ConnectDialogState,
    events: &Sender<AppEvent>,
    actions: &mut DialogActions,
) {
    // --- Pairing state ---
    ui.horizontal(|ui| {
        ui.strong("Pairing status:");
        let (text, color) = match state.pairing {
            PairingState::Idle => ("Not paired".to_string(), egui::Color32::GRAY),
            PairingState::Pairing => ("Pairing…".to_string(), egui::Color32::LIGHT_BLUE),
            PairingState::Paired => ("Paired ✓".to_string(), egui::Color32::GREEN),
            PairingState::Failed => ("Pairing failed".to_string(), egui::Color32::RED),
        };
        ui.colored_label(color, text);
    });
    if let Some(msg) = state.pairing_message.clone() {
        ui.monospace(msg);
    }
    if state.pairing == PairingState::Paired {
        ui.colored_label(
            egui::Color32::GREEN,
            "Paired! Now open the Manual tab and connect with the ADB \
             connection port (it differs from the pairing port).",
        );
    }
    ui.separator();

    // --- QR scan ---
    ui.strong("1. Scan the phone's QR code (fills in the pairing code)");
    ui.label("On the phone: Developer options → Wireless debugging → Pair device with QR code.");
    qr_section(ctx, ui, dlg, events);
    ui.separator();

    // --- Pairing-code form ---
    ui.strong("2. Pair with IP + pairing port + code");
    ui.label(
        "Use the *pairing* port from the phone dialog (e.g. 37001) — not the connection port.",
    );
    ui.horizontal(|ui| {
        ui.label("IP address");
        ui.text_edit_singleline(&mut dlg.pair_ip);
    });
    ui.horizontal(|ui| {
        ui.label("Pairing port");
        ui.text_edit_singleline(&mut dlg.pair_port);
    });
    ui.horizontal(|ui| {
        ui.label("Pairing code");
        ui.text_edit_singleline(&mut dlg.pair_code);
    });
    if let Some(err) = dlg.pair_form_error.clone() {
        ui.colored_label(egui::Color32::YELLOW, err);
    }
    ui.add_space(4.0);
    let can_pair = !state.pairing.in_progress() && state.config.adb_path.is_some();
    if ui
        .add_enabled(can_pair, egui::Button::new("Pair device"))
        .clicked()
    {
        match validate_pair_input(&dlg.pair_ip, &dlg.pair_port, &dlg.pair_code) {
            Ok(req) => {
                dlg.pair_form_error = None;
                state.pairing = PairingState::Pairing;
                state.pairing_message = Some(format!("Pairing with {}…", req.addr()));
                actions.pair_request = Some(req);
            }
            Err(msg) => {
                dlg.pair_form_error = Some(msg);
            }
        }
    }
    if state.config.adb_path.is_none() {
        ui.label("Configure ADB in Settings first.");
    }
}

fn qr_section(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    dlg: &mut ConnectDialogState,
    events: &Sender<AppEvent>,
) {
    ui.label("Camera frames are decoded on this PC only — never uploaded.");
    ui.horizontal(|ui| {
        if dlg.camera.is_none() {
            if ui.button("Start camera").clicked() {
                match CameraScanner::open_default() {
                    Ok(camera) => {
                        dlg.camera = Some(camera);
                        dlg.qr_status =
                            Some("Point the phone QR code inside the preview.".to_string());
                    }
                    Err(e) => {
                        dlg.qr_status = Some(format!(
                            "{e} Use manual pairing below or load a QR screenshot."
                        ));
                    }
                }
            }
            if ui.button("Load QR image…").clicked() {
                load_qr_image(dlg, events);
            }
        } else if ui.button("Stop camera").clicked() {
            dlg.camera = None;
        }
    });

    if dlg.camera.is_some() {
        // Keep frames flowing while the preview is open.
        ctx.request_repaint();
        pump_camera_frame(ctx, ui, dlg, events);
    } else if let Some(tex) = &dlg.qr_texture {
        ui.image((tex.id(), egui::vec2(320.0, 240.0)));
    }

    if let Some(s) = dlg.qr_status.clone() {
        ui.monospace(s);
    }
}

/// Grab one camera frame, refresh the preview texture, and attempt a decode
/// every 10th frame (decode is milliseconds-cheap at preview resolution).
fn pump_camera_frame(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    dlg: &mut ConnectDialogState,
    events: &Sender<AppEvent>,
) {
    let frame = match dlg.camera.as_mut().map(|c| c.next_frame()) {
        Some(Ok(frame)) => frame,
        Some(Err(e)) => {
            dlg.qr_status = Some(format!("Camera error: {e}"));
            dlg.camera = None;
            return;
        }
        None => return,
    };

    let color = egui::ColorImage::from_rgba_unmultiplied(
        [frame.width as usize, frame.height as usize],
        &frame.rgba,
    );
    match dlg.qr_texture.as_mut() {
        Some(tex) => tex.set(color, egui::TextureOptions::LINEAR),
        None => {
            dlg.qr_texture =
                Some(ctx.load_texture("qr-preview", color, egui::TextureOptions::LINEAR));
        }
    }

    dlg.qr_frames += 1;
    if dlg.qr_frames % 10 == 0 {
        if let Some(raw) =
            image::ImageBuffer::from_raw(frame.width, frame.height, frame.rgba.clone())
        {
            let gray = image::DynamicImage::ImageRgba8(raw).to_luma8();
            match decode_first_qr(&gray) {
                Ok(text) => handle_scanned_text(dlg, events, &text),
                Err(crate::pairing::QrError::NoQrFound) => {}
                Err(e) => {
                    dlg.qr_status = Some(format!("{e} {}", e.guidance()));
                }
            }
        }
    }

    if let Some(tex) = &dlg.qr_texture {
        ui.image((tex.id(), egui::vec2(320.0, 240.0)));
    }
}

fn load_qr_image(dlg: &mut ConnectDialogState, events: &Sender<AppEvent>) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("images", &["png", "jpg", "jpeg", "bmp"])
        .set_title("Select a QR code screenshot")
        .pick_file()
    else {
        return;
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            dlg.qr_status = Some(format!("Could not read file: {e}"));
            return;
        }
    };
    match decode_wireless_qr(&bytes) {
        Ok(qr) => {
            dlg.pair_code = qr.password.clone();
            dlg.qr_status = Some(format!("{qr} — pairing code filled in."));
            let _ = events.send(AppEvent::Toast(Toast::success(format!(
                "QR decoded ({qr})"
            ))));
        }
        Err(e) => {
            dlg.qr_status = Some(format!("{e} {}", e.guidance()));
        }
    }
}

/// A camera frame (or file) yielded text: accept wireless codes, reject the
/// rest with guidance, and stop the camera on success.
fn handle_scanned_text(dlg: &mut ConnectDialogState, events: &Sender<AppEvent>, text: &str) {
    match parse_wireless_qr(text) {
        Ok(qr) => {
            dlg.pair_code = qr.password.clone();
            dlg.qr_status = Some(format!("{qr} — pairing code filled in. Camera stopped."));
            dlg.camera = None; // stop after successful pairing scan
            let _ = events.send(AppEvent::Toast(Toast::success(format!(
                "QR decoded ({qr})"
            ))));
        }
        Err(e) => {
            dlg.qr_status = Some(format!("{e} {}", e.guidance()));
        }
    }
}

fn manual_tab(
    ui: &mut egui::Ui,
    state: &mut AppState,
    dlg: &mut ConnectDialogState,
    events: &Sender<AppEvent>,
) {
    ui.label("Connect to an already-paired device with its ADB connection port.");
    ui.horizontal(|ui| {
        ui.label("IP address");
        ui.text_edit_singleline(&mut dlg.ip);
    });
    ui.horizontal(|ui| {
        ui.label("ADB port");
        ui.text_edit_singleline(&mut dlg.port);
    });
    ui.add_space(4.0);
    let can_go = !dlg.busy && !dlg.ip.trim().is_empty() && !dlg.port.trim().is_empty();
    if ui
        .add_enabled(can_go, egui::Button::new("Connect"))
        .clicked()
    {
        let addr = format!("{}:{}", dlg.ip.trim(), dlg.port.trim());
        dlg.busy = true;
        dlg.status = Some(format!("Connecting to {addr}…"));
        let result = state
            .config
            .adb_path
            .clone()
            .map(|p| AdbClient::new(p).connect(&addr));
        dlg.busy = false;
        match result {
            None => {
                dlg.status = Some("No ADB path configured.".to_string());
            }
            Some(Ok(out)) => {
                dlg.status = Some(out.clone());
                if state.config.remember_devices {
                    state.saved.remember_wireless(&addr, None);
                }
                let _ = events.send(AppEvent::Toast(Toast::success(format!(
                    "Connected to {addr}"
                ))));
            }
            Some(Err(e)) => {
                dlg.status = Some(format!(
                    "{}\n{}",
                    e.guidance(),
                    crate::adb::parser::humanize_adb_error(&e.to_string())
                ));
                state.set_adb_error(&e);
            }
        }
    }
    if let Some(s) = dlg.status.clone() {
        ui.separator();
        ui.monospace(s);
    }
}
