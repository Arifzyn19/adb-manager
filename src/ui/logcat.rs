//! Realtime Logcat viewer: toolbar, crash banner + analyzer, dark
//! level-coded virtualized list, footer stats.

use crate::logcat::{entry_matches, CrashReport, LogLevel};
use crate::state::{AppState, LogcatBufferState};
use crate::ui::components::{self, page_header};
use crate::ui::theme::palette;

/// Max rows rendered per frame; the buffer itself holds up to 10k+.
const MAX_RENDER_ROWS: usize = 1500;

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut AppState) {
    page_header(ui, "Logcat", "Realtime device logs with crash detection.");

    let Some(device) = state.selected_device().cloned() else {
        components::no_device_state(ui, state);
        return;
    };
    if !device.state.is_usable() {
        ui.label(
            egui::RichText::new(format!(
                "Logcat unavailable while the device is '{}'.",
                device.state.label()
            ))
            .color(palette::TEXT_DIM),
        );
        return;
    }
    let serial = device.serial.clone();

    // Ensure a buffer exists even before the first batch arrives.
    let capacity = state.config.log_buffer_size;
    state
        .logcat
        .entry(serial.clone())
        .or_insert_with(|| LogcatBufferState::new(capacity));

    ui.horizontal(|ui| {
        stream_status(ui, state);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let paused = state.logcat.get(&serial).is_some_and(|b| b.paused);
            if components::secondary_button(ui, if paused { "▶ Resume" } else { "❚❚ Pause" })
                .clicked()
            {
                if let Some(buf) = state.logcat.get_mut(&serial) {
                    buf.paused = !buf.paused;
                    if !buf.paused {
                        buf.skipped_while_paused = 0;
                    }
                }
            }
            if components::secondary_button(ui, "Clear").clicked() {
                if let Some(buf) = state.logcat.get_mut(&serial) {
                    buf.buffer.clear();
                    buf.skipped_while_paused = 0;
                }
            }
            if components::secondary_button(ui, "Export…").clicked() {
                export_logs(state, &serial);
            }
        });
    });

    toolbar(ui, state, &serial);
    crashes_section(ctx, ui, state, &serial);
    log_list(ui, state, &serial);
    footer(ui, state, &serial);
}

fn stream_status(ui: &mut egui::Ui, state: &AppState) {
    let Some(serial) = state.selected_serial.clone() else {
        components::status_badge(ui, "No device", palette::TEXT_FAINT);
        return;
    };
    match state.logcat.get(&serial) {
        Some(buf) if buf.paused => {
            components::status_badge(ui, "Paused", palette::WARNING);
        }
        Some(_) => {
            components::status_badge(ui, "Streaming", palette::SUCCESS);
        }
        None => {
            components::status_badge(ui, "Starting", palette::TEXT_FAINT);
        }
    }
}

fn toolbar(ui: &mut egui::Ui, state: &mut AppState, serial: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Search")
                .small()
                .color(palette::TEXT_DIM),
        );
        if let Some(buf) = state.logcat.get_mut(serial) {
            ui.add(
                egui::TextEdit::singleline(&mut buf.filter.search)
                    .hint_text("tag, pid or message…")
                    .desired_width(200.0),
            );
        }
        ui.label(
            egui::RichText::new("Level")
                .small()
                .color(palette::TEXT_DIM),
        );
        level_combo(ui, state, serial);
        ui.label(
            egui::RichText::new("Package")
                .small()
                .color(palette::TEXT_DIM),
        );
        package_combo(ui, state, serial);
        if let Some(buf) = state.logcat.get_mut(serial) {
            ui.add(
                egui::TextEdit::singleline(&mut buf.filter.package)
                    .hint_text("com.example.app")
                    .desired_width(180.0),
            );
        }
        if components::secondary_button(ui, "Clear filters").clicked() {
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
        .width(200.0)
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
        ui.colored_label(palette::ERROR, "⚠");
        ui.label(egui::RichText::new(format!("Crashes ({})", crashes.len())).strong());
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
    components::panel(ui, |ui| {
        ui.horizontal(|ui| {
            ui.colored_label(palette::ERROR, "⚠");
            ui.label(
                egui::RichText::new(format!(
                    "{} — {}",
                    report.reason.label(),
                    report.short_exception()
                ))
                .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if components::secondary_button(ui, "Close").clicked() {
                    if let Some(buf) = state.logcat.get_mut(serial) {
                        buf.selected_crash = None;
                    }
                }
            });
        });
        components::kv_grid(
            ui,
            &format!("crash-{}", report.id),
            &[
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
            ],
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if let Some(buf) = state.logcat.get_mut(serial) {
                ui.checkbox(&mut buf.hide_system_frames, "Filter system frames");
            }
            if components::secondary_button(ui, "Copy stacktrace").clicked() {
                let hide = state
                    .logcat
                    .get(serial)
                    .is_some_and(|b| b.hide_system_frames);
                ctx.copy_text(report.to_text(hide));
            }
            if components::secondary_button(ui, "Export…").clicked() {
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
                        ui.colored_label(palette::ERROR, format!("Export failed: {e}"));
                    }
                }
            }
        });
        let hide = state
            .logcat
            .get(serial)
            .is_some_and(|b| b.hide_system_frames);
        let stack = report.visible_stack(hide);
        ui.label(
            egui::RichText::new(format!("Stack trace ({} shown):", stack.len()))
                .small()
                .color(palette::TEXT_DIM),
        );
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                components::sunken_panel(ui, |ui| {
                    for line in &stack {
                        let is_app = !crate::logcat::system_frame(line)
                            && line.trim_start().starts_with("at ");
                        if is_app {
                            ui.label(
                                egui::RichText::new(*line)
                                    .monospace()
                                    .strong()
                                    .color(palette::ERROR),
                            );
                        } else {
                            ui.monospace(*line);
                        }
                    }
                });
            });
    });
}

// --- Log list --------------------------------------------------------------

fn log_list(ui: &mut egui::Ui, state: &mut AppState, serial: &str) {
    ui.separator();
    let Some(buf) = state.logcat.get(serial) else {
        components::loading_state(
            ui,
            "Starting Logcat",
            "Spawning adb logcat on the device…",
            None,
        );
        return;
    };
    if buf.buffer.entries.is_empty() {
        if buf.paused {
            ui.label(
                egui::RichText::new("Paused. Resume to keep receiving lines.")
                    .color(palette::TEXT_DIM),
            );
        } else {
            components::loading_state(ui, "Waiting for Logcat", "No lines received yet…", None);
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
        ui.label(
            egui::RichText::new("No lines match the current filters.").color(palette::TEXT_DIM),
        );
        return;
    }

    let start = filtered.len().saturating_sub(MAX_RENDER_ROWS);
    let truncated = start > 0;
    let autoscroll = state.config.log_auto_scroll && !buf.paused;

    egui::ScrollArea::both()
        .stick_to_bottom(autoscroll)
        .show(ui, |ui| {
            if truncated {
                ui.label(
                    egui::RichText::new(format!(
                        "… showing newest {MAX_RENDER_ROWS} of {} matching lines",
                        filtered.len()
                    ))
                    .small()
                    .color(palette::TEXT_FAINT),
                );
            }
            for entry in &filtered[start..] {
                log_row(ui, entry);
            }
        });
}

fn log_row(ui: &mut egui::Ui, entry: &crate::logcat::LogEntry) {
    if !entry.parsed {
        ui.label(
            egui::RichText::new(&entry.raw)
                .monospace()
                .color(palette::TEXT_FAINT),
        );
        return;
    }
    let (badge, color, bold) = match entry.level {
        LogLevel::Verbose => ("V", palette::LOG_VERBOSE, false),
        LogLevel::Debug => ("D", palette::LOG_DEBUG, false),
        LogLevel::Info => ("I", palette::LOG_INFO, false),
        LogLevel::Warning => ("W", palette::LOG_WARN, false),
        LogLevel::Error => ("E", palette::LOG_ERROR, true),
        LogLevel::Fatal => ("F", palette::LOG_ERROR, true),
        LogLevel::Unknown => ("?", palette::TEXT_FAINT, false),
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            egui::RichText::new(entry.timestamp.clone())
                .monospace()
                .small()
                .color(palette::TEXT_FAINT),
        );
        let level = egui::RichText::new(badge).monospace().strong().color(color);
        ui.add_sized([14.0, 16.0], egui::Label::new(level));
        ui.label(
            egui::RichText::new(format!("{:>5}", entry.pid))
                .monospace()
                .small()
                .color(palette::TEXT_DIM),
        );
        ui.label(
            egui::RichText::new(entry.tag.clone())
                .monospace()
                .color(palette::ACCENT_BRIGHT),
        );
        let msg = egui::RichText::new(entry.message.clone()).monospace();
        ui.label(if bold {
            msg.strong().color(color)
        } else {
            msg.color(palette::TEXT)
        });
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
        ui.label(egui::RichText::new(text).small().color(palette::TEXT_FAINT));
    }
}
