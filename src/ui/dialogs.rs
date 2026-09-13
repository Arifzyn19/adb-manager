//! Connect Device dialog: USB overview, wireless QR + pairing-code flows
//! with explicit pairing state, manual `adb connect`.

use crate::adb::AdbClient;
use crate::events::{AppEvent, Toast};
use crate::pairing::{
    decode_first_qr, decode_wireless_qr, parse_wireless_qr, validate_pair_input, CameraScanner,
    PairingRequest, PairingState,
};
use crate::state::AppState;
use crate::ui::components;
use crate::ui::theme::palette;
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
        .default_width(520.0)
        .show(ctx, |ui| {
            components::segmented(
                ui,
                &[
                    (ConnectTab::Usb, "USB"),
                    (ConnectTab::Wireless, "Wireless"),
                    (ConnectTab::Manual, "Manual"),
                ],
                &mut dlg.tab,
            );
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
    ui.label(
        egui::RichText::new("Accept the on-device authorization prompt if asked.")
            .color(palette::TEXT_DIM),
    );
    ui.add_space(4.0);
    components::kv_line(ui, "Devices visible", &state.devices.len().to_string());
    for d in &state.devices {
        ui.horizontal(|ui| {
            ui.colored_label(
                if d.state.is_usable() {
                    palette::SUCCESS
                } else {
                    palette::WARNING
                },
                "●",
            );
            ui.monospace(format!("{}  ({})", d.serial, d.state.label()));
        });
    }
    if state.devices.iter().any(|d| !d.state.is_usable()) {
        ui.add_space(4.0);
        components::warning_line(
            ui,
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
        ui.label(egui::RichText::new("Pairing status:").color(palette::TEXT_DIM));
        let (text, color, tint) = match state.pairing {
            PairingState::Idle => ("NOT PAIRED", palette::TEXT_DIM, palette::PANEL),
            PairingState::Pairing => ("PAIRING", palette::ACCENT_BRIGHT, palette::ACCENT_TINT),
            PairingState::Paired => ("PAIRED", palette::SUCCESS, palette::SUCCESS_TINT),
            PairingState::Failed => ("FAILED", palette::ERROR, palette::ERROR_TINT),
        };
        components::pill(ui, text, color, tint);
    });
    if let Some(msg) = state.pairing_message.clone() {
        ui.monospace(msg);
    }
    if state.pairing == PairingState::Paired {
        ui.label(
            egui::RichText::new(
                "Paired! Now open the Manual tab and connect with the ADB \
                 connection port (it differs from the pairing port).",
            )
            .color(palette::SUCCESS),
        );
    }
    ui.separator();

    // --- QR scan ---
    components::section_title(ui, "1 · SCAN THE PHONE'S QR CODE");
    ui.label(
        egui::RichText::new(
            "On the phone: Developer options → Wireless debugging → Pair device with QR code.",
        )
        .small()
        .color(palette::TEXT_DIM),
    );
    qr_section(ctx, ui, dlg, events);
    ui.separator();

    // --- Pairing-code form ---
    components::section_title(ui, "2 · PAIR WITH CODE");
    ui.label(
        egui::RichText::new(
            "Use the pairing port from the phone dialog (e.g. 37001) — not the connection port.",
        )
        .small()
        .color(palette::TEXT_DIM),
    );
    components::field_row(ui, "IP address", &mut dlg.pair_ip, "192.168.1.20");
    components::field_row(ui, "Pairing port", &mut dlg.pair_port, "37001");
    components::field_row(ui, "Pairing code", &mut dlg.pair_code, "6-digit code");
    if let Some(err) = dlg.pair_form_error.clone() {
        components::warning_line(ui, &err);
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
        ui.label(egui::RichText::new("Configure ADB in Settings first.").color(palette::TEXT_DIM));
    }
}

fn qr_section(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    dlg: &mut ConnectDialogState,
    events: &Sender<AppEvent>,
) {
    ui.label(
        egui::RichText::new("Camera frames are decoded on this PC only — never uploaded.")
            .small()
            .color(palette::TEXT_FAINT),
    );
    ui.horizontal(|ui| {
        if dlg.camera.is_none() {
            if components::secondary_button(ui, "Start camera").clicked() {
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
            if components::secondary_button(ui, "Load QR image…").clicked() {
                load_qr_image(dlg, events);
            }
        } else if components::secondary_button(ui, "Stop camera").clicked() {
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

    // Scan-area frame with corner emphasis.
    egui::Frame::new()
        .fill(palette::BG_SUNKEN)
        .stroke(egui::Stroke::new(1.0, palette::ACCENT))
        .corner_radius(4.0)
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("SCAN AREA — hold the QR code inside")
                    .small()
                    .color(palette::ACCENT_BRIGHT),
            );
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
            if let Some(tex) = &dlg.qr_texture {
                ui.image((tex.id(), egui::vec2(320.0, 240.0)));
            }
        });

    dlg.qr_frames += 1;
    if dlg.qr_frames.is_multiple_of(10) {
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
    ui.label(
        egui::RichText::new(
            "This is the connection port from Wireless debugging — not the pairing port.",
        )
        .small()
        .color(palette::TEXT_DIM),
    );
    components::field_row(ui, "IP address", &mut dlg.ip, "192.168.1.20");
    components::field_row(ui, "ADB port", &mut dlg.port, "5555");
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
