//! File Manager page: breadcrumb explorer with upload / download /
//! delete / rename / mkdir. Mutations run on worker threads via actions.

use crate::files::{parent_dir, FileKind};
use crate::state::AppState;
use crate::ui::components::{self, page_header};
use crate::ui::theme::palette;

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

    page_header(ui, "Files", "Android storage browser.");

    let Some(device) = state.selected_device().cloned() else {
        components::no_device_state(ui, state);
        return actions;
    };
    if !device.state.is_usable() {
        ui.label(
            egui::RichText::new(format!(
                "File browser unavailable while the device is '{}'.",
                device.state.label()
            ))
            .color(palette::TEXT_DIM),
        );
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
        ui.label(
            egui::RichText::new("Drop files here to upload them.")
                .color(palette::ACCENT_BRIGHT)
                .strong(),
        );
    }
    if !dropped.is_empty() {
        actions.op = Some(FileOp::Upload(dropped));
    }

    // Toolbar: breadcrumb, home/up, upload, refresh.
    let cwd = if state.files.cwd.is_empty() {
        state.config.files_root.clone()
    } else {
        state.files.cwd.clone()
    };
    ui.horizontal(|ui| {
        if components::secondary_button(ui, "⌂").clicked() {
            actions.navigate = Some(state.config.files_root.clone());
        }
        if components::secondary_button(ui, "⬆ Up").clicked() {
            actions.navigate = Some(parent_dir(&cwd));
        }
        if components::secondary_button(ui, "Refresh").clicked() {
            actions.refresh = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::primary_button(ui, "Upload…").clicked() {
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
    breadcrumb(ui, &cwd, &mut actions);
    ui.horizontal(|ui| {
        ui.monospace(&cwd);
        if state.files.loading {
            ui.spinner();
            ui.label(
                egui::RichText::new("Reading…")
                    .small()
                    .color(palette::TEXT_DIM),
            );
        } else if let Some(busy) = state.files.busy.clone() {
            ui.spinner();
            ui.label(
                egui::RichText::new(format!("Working… ({busy})"))
                    .small()
                    .color(palette::TEXT_DIM),
            );
        }
    });

    if let Some(err) = state.files.error.clone() {
        components::error_panel(ui, &err, None);
        ui.horizontal(|ui| {
            if components::secondary_button(ui, "Retry").clicked() {
                actions.refresh = true;
            }
            if components::secondary_button(ui, "Go to root").clicked() {
                actions.navigate = Some(state.config.files_root.clone());
            }
        });
    }

    // New-folder row.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("New folder").color(palette::TEXT_DIM));
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.files.mkdir_name)
                .hint_text("Folder name…")
                .desired_width(200.0),
        );
        let name = state.files.mkdir_name.trim().to_string();
        let can_create = !name.is_empty() && state.files.busy.is_none();
        let create = ui
            .add_enabled(can_create, egui::Button::new("Create"))
            .clicked()
            || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && can_create);
        if create {
            state.files.mkdir_name.clear();
            actions.op = Some(FileOp::Mkdir(name));
        }
    });

    components::search_field(ui, &mut state.files.search, "Search this folder…");
    ui.add_space(4.0);

    let query = state.files.search.to_lowercase();
    let rows: Vec<crate::files::FileEntry> = state
        .files
        .entries
        .iter()
        .filter(|e| query.is_empty() || e.name.to_lowercase().contains(&query))
        .cloned()
        .collect();

    if rows.is_empty() && !state.files.loading {
        ui.label(
            egui::RichText::new(if state.files.entries.is_empty() {
                "This directory is empty."
            } else {
                "No files match the current search."
            })
            .color(palette::TEXT_DIM),
        );
    } else {
        ui.label(
            egui::RichText::new(format!(
                "{} item{}",
                rows.len(),
                if rows.len() == 1 { "" } else { "s" }
            ))
            .small()
            .color(palette::TEXT_DIM),
        );
        components::table_header(
            ui,
            &[
                ("", 24.0),
                ("Name", 260.0),
                ("Size", 80.0),
                ("Type", 70.0),
                ("Modified", 130.0),
            ],
        );
        egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in &rows {
                file_row(ui, state, entry, &mut actions);
            }
        });
    }

    // Two-step delete confirmation (destructive ⇒ ask first, per settings).
    if let Some(target) = state.files.confirm_delete.clone() {
        let msg = format!("Delete {target}?");
        match components::confirm_modal(
            ctx,
            "file-delete",
            "Confirm Delete",
            &[
                (&msg, true),
                ("Folders delete recursively.", false),
                ("This action cannot be undone.", false),
            ],
            "Delete",
            true,
        ) {
            Some(true) => {
                state.files.confirm_delete = None;
                actions.op = Some(FileOp::Delete(target));
            }
            Some(false) => {
                state.files.confirm_delete = None;
            }
            None => {}
        }
    }

    actions
}

fn breadcrumb(ui: &mut egui::Ui, cwd: &str, actions: &mut FilesActions) {
    let parts: Vec<&str> = cwd.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let mut acc = String::new();
        for (i, part) in parts.iter().enumerate() {
            acc.push('/');
            acc.push_str(part);
            if i + 1 < parts.len() {
                let target = acc.clone();
                if ui
                    .selectable_label(false, *part)
                    .on_hover_text(format!("Open {target}"))
                    .clicked()
                {
                    actions.navigate = Some(target);
                }
                ui.label(egui::RichText::new("/").color(palette::TEXT_FAINT));
            } else {
                ui.label(egui::RichText::new(*part).strong());
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
            if components::primary_button(ui, "OK").clicked() {
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
            if components::secondary_button(ui, "Cancel").clicked() {
                state.files.rename_target = None;
            }
        });
        return;
    }

    ui.horizontal(|ui| {
        if busy {
            ui.disable();
        }
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
        ui.add_sized(
            [80.0, 18.0],
            egui::Label::new(
                egui::RichText::new(entry.size_display())
                    .monospace()
                    .color(palette::TEXT_DIM),
            ),
        );
        ui.add_sized(
            [70.0, 18.0],
            egui::Label::new(
                egui::RichText::new(entry.kind.label())
                    .small()
                    .color(palette::TEXT_DIM),
            ),
        );
        ui.add_sized(
            [130.0, 18.0],
            egui::Label::new(
                egui::RichText::new(entry.modified.clone())
                    .monospace()
                    .small()
                    .color(palette::TEXT_FAINT),
            )
            .truncate(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::danger_button(ui, "Delete").clicked() {
                if state.config.confirm_destructive {
                    state.files.confirm_delete = Some(entry.path.clone());
                } else {
                    actions.op = Some(FileOp::Delete(entry.path.clone()));
                }
            }
            if components::secondary_button(ui, "Rename").clicked() {
                state.files.rename_target = Some(entry.path.clone());
                state.files.rename_new = entry.name.clone();
            }
            if components::secondary_button(ui, "Download").clicked() {
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
            if entry.kind == FileKind::Dir && components::secondary_button(ui, "Open").clicked() {
                actions.navigate = Some(entry.path.clone());
            }
        });
    });
}
