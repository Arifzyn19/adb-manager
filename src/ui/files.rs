//! File Manager page (Phase 8): storage browser rooted at `/sdcard/` with
//! upload / download / delete / rename / mkdir. Mutations and transfers run
//! on worker threads; this module only renders state and returns actions.

use crate::files::{parent_dir, FileKind};
use crate::state::AppState;
use crate::ui::theme::StatusColors;

/// One user intent from the page; executed by `app.rs` on worker threads.
pub enum FileOp {
    Mkdir(String),
    Delete(String),
    Rename { from: String, to: String },
    Upload(Vec<String>),
    Download { remote: String, local_dir: String },
}

#[derive(Default)]
pub struct FilesActions {
    pub navigate: Option<String>,
    pub refresh: bool,
    pub op: Option<FileOp>,
}

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut AppState) -> FilesActions {
    let mut actions = FilesActions::default();

    ui.horizontal(|ui| {
        ui.heading("Files");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Refresh").clicked() {
                actions.refresh = true;
            }
            if ui.button("Upload…").clicked() {
                if let Some(paths) = rfd::FileDialog::new()
                    .set_title("Select files to upload")
                    .pick_files()
                {
                    let locals: Vec<String> =
                        paths.iter().map(|p| p.display().to_string()).collect();
                    if !locals.is_empty() {
                        actions.op = Some(FileOp::Upload(locals));
                    }
                }
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
            "File browser unavailable while the device is '{}'.",
            device.state.label()
        ));
        return actions;
    }

    // Windows → Android drag & drop uploads into the current folder.
    let dropped: Vec<String> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter_map(|f| f.path.clone())
            .map(|p| p.display().to_string())
            .collect()
    });
    if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
        ui.colored_label(StatusColors::accent(), "Drop files here to upload them.");
    }
    if !dropped.is_empty() {
        actions.op = Some(FileOp::Upload(dropped));
    }

    // Breadcrumb: Home (configured root) + Up + clickable segments.
    let cwd = if state.files.cwd.is_empty() {
        state.config.files_root.clone()
    } else {
        state.files.cwd.clone()
    };
    ui.horizontal_wrapped(|ui| {
        if ui.small_button("⌂").clicked() {
            actions.navigate = Some(state.config.files_root.clone());
        }
        if ui.small_button("⬆ Up").clicked() {
            actions.navigate = Some(parent_dir(&cwd));
        }
        ui.monospace(&cwd);
        if state.files.loading {
            ui.spinner();
            ui.label("Reading…");
        } else if let Some(busy) = state.files.busy.clone() {
            ui.spinner();
            ui.label(format!("Working… ({busy})"));
        }
    });
    breadcrumb(ui, &cwd, &mut actions);

    if let Some(err) = state.files.error.clone() {
        ui.colored_label(StatusColors::error(), format!("✕ {err}"));
        ui.horizontal(|ui| {
            if ui.button("Retry").clicked() {
                actions.refresh = true;
            }
            if ui.button("Go to root").clicked() {
                actions.navigate = Some(state.config.files_root.clone());
            }
        });
    }

    // New-folder row.
    ui.horizontal(|ui| {
        ui.label("New folder");
        let resp = ui.text_edit_singleline(&mut state.files.mkdir_name);
        let name = state.files.mkdir_name.trim().to_string();
        ui.set_enabled(!name.is_empty() && state.files.busy.is_none());
        let create = ui.button("Create").clicked()
            || (resp.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !name.is_empty());
        if create {
            state.files.mkdir_name.clear();
            actions.op = Some(FileOp::Mkdir(name));
        }
    });

    // Search.
    ui.horizontal(|ui| {
        ui.label("Search");
        ui.text_edit_singleline(&mut state.files.search);
    });

    let query = state.files.search.to_lowercase();
    let rows: Vec<crate::files::FileEntry> = state
        .files
        .entries
        .iter()
        .filter(|e| query.is_empty() || e.name.to_lowercase().contains(&query))
        .cloned()
        .collect();

    if rows.is_empty() && !state.files.loading {
        ui.add_space(8.0);
        if state.files.entries.is_empty() {
            ui.label("This directory is empty.");
        } else {
            ui.label("No files match the current search.");
        }
        delete_modal(ctx, state, &mut actions);
        return actions;
    }

    ui.label(format!(
        "{} item{}",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    ));
    egui::ScrollArea::vertical().show(ui, |ui| {
        for entry in &rows {
            file_row(ui, state, entry, &mut actions);
        }
    });

    delete_modal(ctx, state, &mut actions);
    actions
}

fn breadcrumb(ui: &mut egui::Ui, cwd: &str, actions: &mut FilesActions) {
    let parts: Vec<&str> = cwd.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        let mut acc = String::new();
        for (i, part) in parts.iter().enumerate() {
            acc.push('/');
            acc.push_str(part);
            if i + 1 < parts.len() {
                let target = acc.clone();
                if ui.small_button(*part).clicked() {
                    actions.navigate = Some(target);
                }
                ui.label("/");
            } else {
                ui.strong(*part);
            }
        }
    });
}

fn file_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    entry: &crate::files::FileEntry,
    actions: &mut FilesActions,
) {
    let busy = state.files.busy.is_some();
    // Inline rename editor takes over the row.
    if state.files.rename_target.as_deref() == Some(entry.path.as_str()) {
        ui.horizontal(|ui| {
            ui.label(entry.kind.icon());
            ui.text_edit_singleline(&mut state.files.rename_new);
            if ui.small_button("OK").clicked() {
                let new_name = state.files.rename_new.trim().to_string();
                state.files.rename_target = None;
                if !new_name.is_empty() && new_name != entry.name {
                    let to = parent_dir(&entry.path);
                    let to = crate::files::join_remote(&to, &new_name);
                    actions.op = Some(FileOp::Rename {
                        from: entry.path.clone(),
                        to,
                    });
                }
            }
            if ui.small_button("Cancel").clicked() {
                state.files.rename_target = None;
            }
        });
        return;
    }

    ui.horizontal(|ui| {
        ui.set_enabled(!busy);
        ui.label(entry.kind.icon());
        let mut label = entry.name.clone();
        if entry.kind == FileKind::Symlink {
            if let Some(t) = &entry.link_target {
                label = format!("{} → {t}", entry.name);
            }
        }
        let resp = ui.add_sized(
            [260.0, 18.0],
            egui::Label::new(egui::RichText::new(label).monospace()).truncate(),
        );
        // Double-click (or the Open button) enters folders.
        if entry.kind == FileKind::Dir && resp.double_clicked() {
            actions.navigate = Some(entry.path.clone());
        }
        ui.add_sized([80.0, 18.0], egui::Label::new(entry.size_display()));
        ui.add_sized(
            [90.0, 18.0],
            egui::Label::new(entry.perms.clone()).truncate(),
        );
        ui.monospace(entry.modified.clone());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Delete").clicked() {
                if state.config.confirm_destructive {
                    state.files.confirm_delete = Some(entry.path.clone());
                } else {
                    actions.op = Some(FileOp::Delete(entry.path.clone()));
                }
            }
            if ui.small_button("Rename").clicked() {
                state.files.rename_target = Some(entry.path.clone());
                state.files.rename_new = entry.name.clone();
            }
            if ui.small_button("Download").clicked() {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Choose download destination")
                    .pick_folder()
                {
                    actions.op = Some(FileOp::Download {
                        remote: entry.path.clone(),
                        local_dir: dir.display().to_string(),
                    });
                }
            }
            if entry.kind == FileKind::Dir && ui.small_button("Open").clicked() {
                actions.navigate = Some(entry.path.clone());
            }
        });
    });
}

/// Two-step delete confirmation (destructive ⇒ ask first, per settings).
fn delete_modal(ctx: &egui::Context, state: &mut AppState, actions: &mut FilesActions) {
    let Some(target) = state.files.confirm_delete.clone() else {
        return;
    };
    egui::Window::new("Confirm Delete")
        .collapsible(false)
        .show(ctx, |ui| {
            ui.label("Are you sure you want to delete:");
            ui.monospace(&target);
            ui.colored_label(StatusColors::error(), "This action cannot be undone.");
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    state.files.confirm_delete = None;
                }
                if ui.button("Delete").clicked() {
                    state.files.confirm_delete = None;
                    actions.op = Some(FileOp::Delete(target));
                }
            });
        });
}
