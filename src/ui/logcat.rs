//! Realtime Logcat viewer (Phase 5): toolbar, virtualized list, export,
//! crash banner + crash analyzer.

use crate::logcat::{entry_matches, CrashReport, LogLevel};
use crate::state::{AppState, LogcatBufferState};
use crate::ui::theme::StatusColors;

/// Max rows rendered per frame; the buffer itself holds up to 10k+.
const MAX_RENDER_ROWS: usize = 1500;

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.heading("Logcat");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            stream_status(ui, state);
        });
    });

    let Some(device) = state.selected_device().cloned() else {
        ui.add_space(6.0);
        ui.label("No Android device connected.");
        ui.label("Connect a device using USB or Wireless ADB.");
        if ui.button("Connect Device").clicked() {
            state.show_connect_dialog = true;
        }
        return;
    };
    if !device.state.is_usable() {
        ui.label(format!(
            "Logcat unavailable while the device is '{}'.",
            device.state.label()
        ));
        return;
    }
    let serial = device.serial.clone();

    // Ensure a buffer exists even before the first batch arrives.
    let capacity = state.config.log_buffer_size;
    state
        .logcat
        .entry(serial.clone())
        .or_insert_with(|| LogcatBufferState::new(capacity));

    toolbar(ui, state, &serial);
    crashes_section(ctx, ui, state, &serial);
    log_list(ui, state, &serial);
    footer(ui, state, &serial);
}

fn stream_status(ui: &mut egui::Ui, state: &AppState) {
    let Some(serial) = state.selected_serial.clone() else {
        ui.label("○ No device");
        return;
    };
    match state.logcat.get(&serial) {
        Some(buf) if buf.paused => {
            ui.colored_label(StatusColors::warning(), "❚❚ Paused");
        }
        Some(_) => {
            ui.colored_label(StatusColors::connected(), "● Streaming");
        }
        None => {
            ui.colored_label(StatusColors::muted(), "… Starting");
        }
    }
}

fn toolbar(ui: &mut egui::Ui, state: &mut AppState, serial: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Search");
        if let Some(buf) = state.logcat.get_mut(serial) {
            ui.text_edit_singleline(&mut buf.filter.search);
        }
        ui.label("Level");
        level_combo(ui, state, serial);
        ui.label("Package");
        package_combo(ui, state, serial);
        if let Some(buf) = state.logcat.get_mut(serial) {
            ui.text_edit_singleline(&mut buf.filter.package);
        }
    });
    ui.horizontal_wrapped(|ui| {
        let paused = state.logcat.get(serial).is_some_and(|b| b.paused);
        if ui
            .button(if paused { "▶ Resume" } else { "❚❚ Pause" })
            .clicked()
        {
            if let Some(buf) = state.logcat.get_mut(serial) {
                buf.paused = !buf.paused;
                if !buf.paused {
                    buf.skipped_while_paused = 0;
                }
            }
        }
        if ui.button("Clear").clicked() {
            if let Some(buf) = state.logcat.get_mut(serial) {
                buf.buffer.clear();
                buf.skipped_while_paused = 0;
            }
        }
        if ui.button("Export…").clicked() {
            export_logs(state, serial);
        }
        if ui.button("Clear filters").clicked() {
            if let Some(buf) = state.logcat.get_mut(serial) {
                buf.filter = crate::logcat::LogViewFilter::default();
            }
        }
    });
}

fn level_combo(ui: &mut egui::Ui, state: &mut AppState, serial: &str) {
    let options = [
        ("All levels", LogLevel::Verbose),
        ("Debug+", LogLevel::Debug),
        ("Info+", LogLevel::Info),
        ("Warning+", LogLevel::Warning),
        ("Error+", LogLevel::Error),
        ("Fatal", LogLevel::Fatal),
    ];
    let current = state
        .logcat
        .get(serial)
        .map(|b| b.filter.min_level)
        .unwrap_or(LogLevel::Verbose);
    let label = options
        .iter()
        .find(|(_, l)| *l == current)
        .map(|(name, _)| *name)
        .unwrap_or("All levels");
    egui::ComboBox::from_id_salt("logcat-level")
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (name, level) in options {
                if ui.selectable_label(current == level, name).clicked() {
                    if let Some(buf) = state.logcat.get_mut(serial) {
                        buf.filter.min_level = level;
                    }
                }
            }
        });
}

fn package_combo(ui: &mut egui::Ui, state: &mut AppState, serial: &str) {
    let mut packages: Vec<String> = state
        .logcat
        .get(serial)
        .map(|b| {
            let mut v: Vec<String> = b.pid_map.values().cloned().collect();
            v.sort();
            v.dedup();
            v
        })
        .unwrap_or_default();
    if packages.is_empty() {
        return;
    }
    packages.insert(0, String::new());
    let current = state
        .logcat
        .get(serial)
        .map(|b| b.filter.package.clone())
        .unwrap_or_default();
    let label = if current.is_empty() {
        "All packages".to_string()
    } else {
        current.clone()
    };
    egui::ComboBox::from_id_salt("logcat-package")
        .selected_text(label)
        .show_ui(ui, |ui| {
            for pkg in &packages {
                let name = if pkg.is_empty() { "All packages" } else { pkg };
                if ui.selectable_label(current == *pkg, name).clicked() {
                    if let Some(buf) = state.logcat.get_mut(serial) {
                        buf.filter.package = pkg.clone();
                    }
                }
            }
        });
}

fn export_logs(state: &mut AppState, serial: &str) {
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(format!("logcat_{serial}.txt"))
        .set_title("Export visible logcat lines")
        .save_file()
    else {
        return;
    };
    let text = if let Some(buf) = state.logcat.get(serial) {
        buf.buffer
            .entries
            .iter()
            .filter(|e| entry_matches(e, &buf.filter, &buf.pid_map))
            .map(|e| e.to_text())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };
    let notice = match std::fs::write(&path, &text) {
        Ok(()) => format!(
            "Exported {} lines to {}",
            text.lines().count(),
            path.display()
        ),
        Err(e) => format!("Export failed: {e}"),
    };
    if let Some(buf) = state.logcat.get_mut(serial) {
        buf.notice = Some(notice);
    }
}

// --- Crashes ---------------------------------------------------------------

fn crashes_section(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut AppState, serial: &str) {
    let crashes: Vec<CrashReport> = state
        .logcat
        .get(serial)
        .map(|b| b.crashes.clone())
        .unwrap_or_default();
    if crashes.is_empty() {
        return;
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.colored_label(StatusColors::error(), "⚠");
        ui.strong(format!("Crashes ({})", crashes.len()));
    });
    egui::ScrollArea::horizontal().show(ui, |ui| {
        ui.horizontal(|ui| {
            for report in crashes.iter().rev().take(10) {
                let label = format!(
                    "{} — {} ({})",
                    report.package,
                    report.short_exception(),
                    report.reason.label()
                );
                let selected =
                    state.logcat.get(serial).and_then(|b| b.selected_crash) == Some(report.id);
                if ui.selectable_label(selected, label).clicked() {
                    if let Some(buf) = state.logcat.get_mut(serial) {
                        buf.selected_crash = Some(report.id);
                    }
                }
            }
        });
    });

    let selected_id = state.logcat.get(serial).and_then(|b| b.selected_crash);
    if let Some(id) = selected_id {
        if let Some(report) = crashes.iter().find(|r| r.id == id).cloned() {
            crash_detail(ctx, ui, state, serial, &report);
        }
    }
}

fn crash_detail(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    state: &mut AppState,
    serial: &str,
    report: &CrashReport,
) {
    egui::Frame::group(ui.style())
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(format!(
                    "{} — {}",
                    report.reason.label(),
                    report.short_exception()
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        if let Some(buf) = state.logcat.get_mut(serial) {
                            buf.selected_crash = None;
                        }
                    }
                });
            });
            egui::Grid::new(format!("crash-{}", report.id))
                .num_columns(2)
                .spacing([12.0, 3.0])
                .striped(true)
                .show(ui, |ui| {
                    for (k, v) in [
                        ("Application", report.package.as_str()),
                        ("Process", report.process.as_str()),
                        ("Exception", report.exception.as_str()),
                        (
                            "Thread",
                            if report.thread.is_empty() {
                                "—"
                            } else {
                                &report.thread
                            },
                        ),
                        (
                            "Timestamp",
                            if report.timestamp.is_empty() {
                                "—"
                            } else {
                                &report.timestamp
                            },
                        ),
                    ] {
                        ui.label(k);
                        ui.monospace(v);
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if let Some(buf) = state.logcat.get_mut(serial) {
                    ui.checkbox(&mut buf.hide_system_frames, "Filter system frames");
                }
                if ui.button("Copy stacktrace").clicked() {
                    let hide = state
                        .logcat
                        .get(serial)
                        .is_some_and(|b| b.hide_system_frames);
                    ctx.copy_text(report.to_text(hide));
                }
                if ui.button("Export…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(format!("crash_{}.txt", report.package))
                        .set_title("Export crash report")
                        .save_file()
                    {
                        let hide = state
                            .logcat
                            .get(serial)
                            .is_some_and(|b| b.hide_system_frames);
                        if let Err(e) = std::fs::write(&path, report.to_text(hide)) {
                            ui.colored_label(StatusColors::error(), format!("Export failed: {e}"));
                        }
                    }
                }
            });
            let hide = state
                .logcat
                .get(serial)
                .is_some_and(|b| b.hide_system_frames);
            let stack = report.visible_stack(hide);
            ui.label(format!("Stack trace ({} shown):", stack.len()));
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for line in &stack {
                        let is_app = !crate::logcat::system_frame(line)
                            && line.trim_start().starts_with("at ");
                        if is_app {
                            ui.label(egui::RichText::new(*line).monospace().strong());
                        } else {
                            ui.monospace(*line);
                        }
                    }
                });
        });
}

// --- Log list --------------------------------------------------------------

fn log_list(ui: &mut egui::Ui, state: &mut AppState, serial: &str) {
    ui.separator();
    let Some(buf) = state.logcat.get(serial) else {
        ui.label("Waiting for Logcat… (starting adb logcat)");
        return;
    };
    if buf.buffer.entries.is_empty() {
        if buf.paused {
            ui.label("Paused. Resume to keep receiving lines.");
        } else {
            ui.label("Waiting for Logcat… (no lines yet)");
        }
        return;
    }

    // Collect matching rows, render only the newest slice.
    let filtered: Vec<&crate::logcat::LogEntry> = buf
        .buffer
        .entries
        .iter()
        .filter(|e| entry_matches(e, &buf.filter, &buf.pid_map))
        .collect();

    if filtered.is_empty() {
        ui.label("No lines match the current filters.");
        return;
    }

    let start = filtered.len().saturating_sub(MAX_RENDER_ROWS);
    let truncated = start > 0;
    let autoscroll = state.config.log_auto_scroll && !buf.paused;

    egui::ScrollArea::both()
        .stick_to_bottom(autoscroll)
        .show(ui, |ui| {
            if truncated {
                ui.label(format!(
                    "… showing newest {MAX_RENDER_ROWS} of {} matching lines",
                    filtered.len()
                ));
            }
            for entry in &filtered[start..] {
                log_row(ui, entry);
            }
        });
}

fn log_row(ui: &mut egui::Ui, entry: &crate::logcat::LogEntry) {
    if !entry.parsed {
        ui.colored_label(StatusColors::muted(), &entry.raw);
        return;
    }
    let color = match entry.level {
        LogLevel::Verbose => StatusColors::muted(),
        LogLevel::Debug => StatusColors::accent(),
        LogLevel::Info => StatusColors::connected(),
        LogLevel::Warning => StatusColors::warning(),
        LogLevel::Error | LogLevel::Fatal => StatusColors::error(),
        LogLevel::Unknown => StatusColors::muted(),
    };
    ui.horizontal(|ui| {
        ui.colored_label(color, entry.level.label());
        ui.monospace(format!(
            "{} {:>5} {}: {}",
            entry.timestamp, entry.pid, entry.tag, entry.message
        ));
    });
}

fn footer(ui: &mut egui::Ui, state: &AppState, serial: &str) {
    ui.separator();
    if let Some(buf) = state.logcat.get(serial) {
        let mut text = format!(
            "{} buffered (ring {})",
            buf.buffer.entries.len(),
            state.config.log_buffer_size
        );
        if buf.buffer.dropped > 0 {
            text.push_str(&format!(" • {} oldest dropped", buf.buffer.dropped));
        }
        if buf.paused && buf.skipped_while_paused > 0 {
            text.push_str(&format!(
                " • {} skipped while paused",
                buf.skipped_while_paused
            ));
        }
        if !buf.crashes.is_empty() {
            text.push_str(&format!(" • {} crash(es)", buf.crashes.len()));
        }
        if let Some(notice) = buf.notice.clone() {
            text.push_str(&format!(" • {notice}"));
        }
        ui.label(text);
    }
}
