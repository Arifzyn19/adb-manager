//! Interactive ADB Shell page: dark terminal panel, distinct command
//! input, history, clear/copy/save.

use crate::state::AppState;
use crate::ui::components::{self, page_header};
use crate::ui::theme::palette;

#[derive(Default)]
pub struct ShellActions {
    pub send: Option<String>,
    pub stop: bool,
}

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut AppState) -> ShellActions {
    let mut actions = ShellActions::default();

    page_header(
        ui,
        "ADB Shell",
        "Persistent shell session on the selected device.",
    );

    let Some(device) = state.selected_device().cloned() else {
        components::no_device_state(ui, state);
        return actions;
    };
    if !device.state.is_usable() {
        ui.label(
            egui::RichText::new(format!(
                "Shell unavailable while the device is '{}'.",
                device.state.label()
            ))
            .color(palette::TEXT_DIM),
        );
        return actions;
    }

    ui.horizontal(|ui| {
        if state.shell.connected {
            components::status_badge(ui, "Session live", palette::SUCCESS);
        } else {
            components::status_badge(ui, "Starting session", palette::WARNING);
        }
        ui.monospace(&device.serial);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::secondary_button(ui, "Save output").clicked() {
                save_transcript(state);
            }
            if components::secondary_button(ui, "Copy all").clicked() {
                ctx.copy_text(transcript_text(state));
            }
            if components::secondary_button(ui, "Clear").clicked() {
                state.shell.blocks.clear();
                state.shell.notice = None;
            }
        });
    });
    ui.label(
        egui::RichText::new("Commands run as the shell user. Interactive prompts (vi, …) will hang — Stop the session instead.")
            .small()
            .color(palette::TEXT_FAINT),
    );
    if let Some(err) = state.shell.error.clone() {
        components::error_panel(ui, &err, None);
    }
    if let Some(notice) = state.shell.notice.clone() {
        ui.label(egui::RichText::new(notice).small().color(palette::TEXT_DIM));
    }

    // Terminal transcript.
    components::sunken_panel(ui, |ui| {
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if state.shell.blocks.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "No commands yet. Try:  getprop ro.build.version.release",
                        )
                        .monospace()
                        .color(palette::TEXT_FAINT),
                    );
                }
                for block in &state.shell.blocks {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}:/ $", device.serial))
                                .monospace()
                                .color(palette::ACCENT_BRIGHT)
                                .strong(),
                        );
                        ui.label(egui::RichText::new(&block.cmd).monospace().strong());
                    });
                    for line in &block.output {
                        ui.monospace(line);
                    }
                    if block.code != 0 {
                        ui.label(
                            egui::RichText::new(format!("↵ exit code {}", block.code))
                                .monospace()
                                .small()
                                .color(palette::WARNING),
                        );
                    }
                }
                if let Some(running) = state.shell.running.clone() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.monospace(format!("running: {running}"));
                    });
                }
            });
    });

    // Distinct command input.
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(palette::PANEL)
        .stroke(egui::Stroke::new(1.0, palette::ACCENT))
        .corner_radius(4.0.into())
        .inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 5,
            bottom: 5,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("$")
                        .monospace()
                        .strong()
                        .color(palette::ACCENT_BRIGHT),
                );
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut state.shell.input)
                        .hint_text("Type a shell command, Enter to send…")
                        .desired_width(f32::INFINITY)
                        .frame(false),
                );
                // History navigation while the input owns focus.
                if resp.has_focus() {
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        step_history(state, true);
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        step_history(state, false);
                    }
                }
                let cmd = state.shell.input.trim().to_string();
                let can_send = !cmd.is_empty() && state.shell.running.is_none();
                let send = ui
                    .add_enabled(can_send, egui::Button::new("Send"))
                    .clicked()
                    || (resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && can_send);
                if send {
                    state.shell.input.clear();
                    state.shell.hist_idx = None;
                    actions.send = Some(cmd);
                }
                if state.shell.running.is_some() && components::danger_button(ui, "Stop").clicked()
                {
                    actions.stop = true;
                }
            });
        });

    actions
}

fn step_history(state: &mut AppState, up: bool) {
    if state.shell.history.is_empty() {
        return;
    }
    let next = match state.shell.hist_idx {
        None if up => Some(state.shell.history.len() - 1),
        None => None,
        Some(i) if up => Some(i.saturating_sub(1)),
        Some(i) => {
            if i + 1 >= state.shell.history.len() {
                None
            } else {
                Some(i + 1)
            }
        }
    };
    state.shell.hist_idx = next;
    state.shell.input = next
        .and_then(|i| state.shell.history.get(i).cloned())
        .unwrap_or_default();
}

fn transcript_text(state: &AppState) -> String {
    let mut out = String::new();
    for block in &state.shell.blocks {
        out.push_str("$ ");
        out.push_str(&block.cmd);
        out.push('\n');
        for line in &block.output {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn save_transcript(state: &mut AppState) {
    let Some(path) = rfd::FileDialog::new()
        .set_file_name("shell_output.txt")
        .set_title("Save shell transcript")
        .save_file()
    else {
        return;
    };
    let text = transcript_text(state);
    state.shell.notice = match std::fs::write(&path, &text) {
        Ok(()) => Some(format!("Saved transcript to {}", path.display())),
        Err(e) => Some(format!("Save failed: {e}")),
    };
}
