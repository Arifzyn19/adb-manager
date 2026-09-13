//! Interactive ADB Shell page (Phase 9).
//!
//! Persistent per-device session owned by `app.rs`; this module renders the
//! transcript and returns send/stop intents. Commands run as the device's
//! shell user — the page says so.

use crate::state::AppState;
use crate::ui::theme::StatusColors;

#[derive(Default)]
pub struct ShellActions {
    pub send: Option<String>,
    pub stop: bool,
}

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut AppState) -> ShellActions {
    let mut actions = ShellActions::default();

    ui.horizontal(|ui| {
        ui.heading("ADB Shell");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Save output").clicked() {
                save_transcript(state);
            }
            if ui.button("Copy all").clicked() {
                ctx.copy_text(transcript_text(state));
            }
            if ui.button("Clear").clicked() {
                state.shell.blocks.clear();
                state.shell.notice = None;
            }
        });
    });

    let Some(device) = state.selected_device().cloned() else {
        ui.add_space(6.0);
        ui.label("No Android device connected.");
        ui.label("Connect a device using USB or Wireless ADB.");
        if ui.button("Connect Device").clicked() {
            state.show_connect_dialog = true;
        }
        return actions;
    };
    if !device.state.is_usable() {
        ui.label(format!(
            "Shell unavailable while the device is '{}'.",
            device.state.label()
        ));
        return actions;
    }

    ui.horizontal(|ui| {
        ui.colored_label(
            if state.shell.connected {
                StatusColors::connected()
            } else {
                StatusColors::warning()
            },
            if state.shell.connected {
                "● session live"
            } else {
                "● session starting…"
            },
        );
        ui.monospace(&device.serial);
        ui.colored_label(
            StatusColors::muted(),
            "Commands run as the shell user. Interactive prompts (vi, …) will hang — Stop the session instead.",
        );
    });
    if let Some(err) = state.shell.error.clone() {
        ui.colored_label(StatusColors::error(), format!("✕ {err}"));
    }
    if let Some(notice) = state.shell.notice.clone() {
        ui.colored_label(StatusColors::muted(), notice);
    }

    // Transcript (auto-scrolls while new blocks arrive).
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if state.shell.blocks.is_empty() {
                ui.colored_label(
                    StatusColors::muted(),
                    "No commands yet. Try: getprop ro.build.version.release",
                );
            }
            for block in &state.shell.blocks {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(StatusColors::accent(), format!("{}:/ $", device.serial));
                    ui.monospace(&block.cmd);
                });
                for line in &block.output {
                    ui.monospace(line);
                }
                if block.code != 0 {
                    ui.colored_label(
                        StatusColors::warning(),
                        format!("(exit code {})", block.code),
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

    // Input row.
    ui.horizontal(|ui| {
        ui.label("$");
        let resp = ui.text_edit_singleline(&mut state.shell.input);
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
        ui.set_enabled(can_send);
        let send = ui.button("Send").clicked()
            || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && can_send);
        if send {
            state.shell.input.clear();
            state.shell.hist_idx = None;
            actions.send = Some(cmd);
        }
        ui.set_enabled(true);
        if state.shell.running.is_some() && ui.button("Stop").clicked() {
            actions.stop = true;
        }
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
