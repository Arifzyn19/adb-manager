//! App Manager page: statistics, searchable table, right-side detail panel
//! with confirm-gated destructive actions.

use crate::apps::{AppFilter, AppInfo};
use crate::state::{AppActionKind, AppDetailTab, AppState, PendingAppAction};
use crate::ui::components::{self, page_header};
use crate::ui::theme::{mono, palette};

#[derive(Default)]
pub struct AppsActions {
    pub refresh_requested: bool,
    /// (package, system, running) — fetch full details on demand.
    pub details_requested: Option<(String, bool, bool)>,
    /// (package, kind) — run an action on a worker thread.
    pub action_requested: Option<(String, AppActionKind)>,
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) -> AppsActions {
    let mut actions = AppsActions::default();

    let Some(device) = state.selected_device().cloned() else {
        page_header(ui, "Apps", "Installed packages on the selected device.");
        components::no_device_state(ui, state);
        return actions;
    };
    if !device.state.is_usable() {
        page_header(ui, "Apps", "Installed packages on the selected device.");
        ui.label(
            egui::RichText::new(format!(
                "App list unavailable while the device is '{}'.",
                device.state.label()
            ))
            .color(palette::TEXT_DIM),
        );
        return actions;
    }

    // Master / detail split: selecting an app docks its panel on the right.
    if state.apps_view.selected.is_some() {
        ui.columns(2, |cols| {
            let col = &mut cols[0];
            page_header(col, "Apps", "Installed packages on the selected device.");
            show_list(col, state, &device.serial, &mut actions);
            let col = &mut cols[1];
            if let Some(pkg) = state.apps_view.selected.clone() {
                show_details(col, state, &device.serial, &pkg, &mut actions);
            }
        });
        return actions;
    }

    page_header(ui, "Apps", "Installed packages on the selected device.");
    show_list(ui, state, &device.serial, &mut actions);
    actions
}

// --- List ------------------------------------------------------------------

fn show_list(ui: &mut egui::Ui, state: &mut AppState, serial: &str, actions: &mut AppsActions) {
    ui.horizontal(|ui| {
        if components::secondary_button(ui, "Refresh").clicked() {
            actions.refresh_requested = true;
        }
        let (loading, resolving, progress) = state
            .apps
            .get(serial)
            .map(|c| (c.list_loading, c.resolving, c.progress))
            .unwrap_or((true, false, (0, 0)));
        if resolving {
            ui.label(
                egui::RichText::new(format!("Resolving {}/{}…", progress.0, progress.1))
                    .small()
                    .color(palette::TEXT_DIM),
            );
        }
        let _ = loading;
    });

    // Statistics row.
    if let Some(cache) = state.apps.get(serial) {
        let total = cache.entries.len();
        let system = cache.entries.iter().filter(|e| e.system).count();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            components::stat_block(ui, "Total", &total.to_string(), "");
            components::stat_block(ui, "Running", &cache.running.len().to_string(), "");
            components::stat_block(ui, "User", &(total - system).to_string(), "");
            components::stat_block(ui, "System", &system.to_string(), "");
        });
        ui.add_space(4.0);
    }

    components::segmented(
        ui,
        &[
            (AppFilter::All, "All"),
            (AppFilter::User, "User"),
            (AppFilter::System, "System"),
            (AppFilter::Running, "Running"),
        ],
        &mut state.apps_view.filter,
    );

    components::search_field(ui, &mut state.apps_view.search, "Search name or package…");
    ui.add_space(4.0);

    // Loading / progress.
    let (loading, _resolving, _progress) = state
        .apps
        .get(serial)
        .map(|c| (c.list_loading, c.resolving, c.progress))
        .unwrap_or((true, false, (0, 0)));
    if loading && state.apps.get(serial).is_none_or(|c| c.entries.is_empty()) {
        components::loading_state(
            ui,
            "Loading installed apps",
            "pm list packages — then labels and versions resolve in the background.",
            None,
        );
        return;
    }

    let rows = filtered_rows(state, serial);
    if rows.is_empty() {
        ui.label(
            egui::RichText::new(if state.apps_view.filter == AppFilter::Running {
                "No running apps detected. The Running tab reads `ps` output; some devices restrict it."
            } else {
                "No applications match the current filter."
            })
            .color(palette::TEXT_DIM),
        );
        return;
    }

    ui.label(
        egui::RichText::new(format!("{} apps", rows.len()))
            .small()
            .color(palette::TEXT_DIM),
    );
    components::table_header(
        ui,
        &[
            ("", 22.0),
            ("Application", 190.0),
            ("Package", 210.0),
            ("Version", 70.0),
            ("State", 90.0),
        ],
    );
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (package, system, info) in &rows {
            app_row(ui, state, package, *system, info.clone(), actions);
        }
    });
}

fn filtered_rows(state: &AppState, serial: &str) -> Vec<(String, bool, Option<AppInfo>)> {
    let query = state.apps_view.search.to_lowercase();
    let filter = state.apps_view.filter;
    let mut rows: Vec<(String, bool, Option<AppInfo>)> = Vec::new();
    if let Some(cache) = state.apps.get(serial) {
        for entry in &cache.entries {
            let running = cache.running.contains(&entry.package);
            match filter {
                AppFilter::All => {}
                AppFilter::User => {
                    if entry.system {
                        continue;
                    }
                }
                AppFilter::System => {
                    if !entry.system {
                        continue;
                    }
                }
                AppFilter::Running => {
                    if !running {
                        continue;
                    }
                }
            }
            let info = cache.resolved.get(&entry.package).cloned();
            if !query.is_empty() {
                let label = info
                    .as_ref()
                    .and_then(|i| i.label.clone())
                    .unwrap_or_default()
                    .to_lowercase();
                if !label.contains(&query) && !entry.package.to_lowercase().contains(&query) {
                    continue;
                }
            }
            rows.push((entry.package.clone(), entry.system, info));
        }
    }
    rows.sort_by(|a, b| {
        let an =
            a.2.as_ref()
                .and_then(|i| i.label.clone())
                .unwrap_or_else(|| a.0.clone());
        let bn =
            b.2.as_ref()
                .and_then(|i| i.label.clone())
                .unwrap_or_else(|| b.0.clone());
        an.to_lowercase().cmp(&bn.to_lowercase())
    });
    rows
}

fn app_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    package: &str,
    system: bool,
    info: Option<AppInfo>,
    actions: &mut AppsActions,
) {
    let selected = state.apps_view.selected.as_deref() == Some(package);
    let (name, version, state_label, state_color) = match &info {
        Some(i) => (
            i.display_name(),
            i.version_name.clone().unwrap_or_else(|| "—".to_string()),
            i.state_label().to_string(),
            match i.state_label() {
                "Running" => palette::SUCCESS,
                "Disabled" => palette::WARNING,
                _ => palette::TEXT_FAINT,
            },
        ),
        None => (
            package.to_string(),
            "…".to_string(),
            (if system { "System" } else { "User" }).to_string(),
            palette::TEXT_FAINT,
        ),
    };

    ui.horizontal(|ui| {
        ui.colored_label(state_color, "●");
        ui.add_sized(
            [190.0, 18.0],
            egui::Label::new(egui::RichText::new(name).strong()).truncate(),
        );
        ui.add_sized(
            [210.0, 18.0],
            egui::Label::new(mono(package.to_string())).truncate(),
        );
        ui.add_sized(
            [70.0, 18.0],
            egui::Label::new(egui::RichText::new(version).color(palette::TEXT_DIM)).truncate(),
        );
        ui.add_sized(
            [90.0, 18.0],
            egui::Label::new(egui::RichText::new(state_label).color(state_color).small()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::secondary_button(ui, if selected { "Close" } else { "Open" }).clicked() {
                if selected {
                    state.apps_view.selected = None;
                } else {
                    state.apps_view.selected = Some(package.to_string());
                    state.apps_view.detail_tab = AppDetailTab::Overview;
                    if info.is_none() {
                        actions.details_requested = Some((package.to_string(), system, false));
                    }
                }
            }
        });
    });
}

// --- Details (right-side panel) ----------------------------------------------

fn show_details(
    ui: &mut egui::Ui,
    state: &mut AppState,
    serial: &str,
    package: &str,
    actions: &mut AppsActions,
) {
    components::panel(ui, |ui| {
        ui.horizontal(|ui| {
            if components::secondary_button(ui, "← Back").clicked() {
                state.apps_view.selected = None;
            }
            ui.label(egui::RichText::new("Application details").strong());
        });
        ui.separator();

        let info = state
            .apps
            .get(serial)
            .and_then(|c| c.resolved.get(package))
            .cloned();
        let Some(info) = info else {
            components::loading_state(ui, "Loading details", "dumpsys package…", None);
            if components::secondary_button(ui, "Retry").clicked() {
                let (system, running) = state
                    .apps
                    .get(serial)
                    .map(|c| {
                        (
                            c.entries
                                .iter()
                                .find(|e| e.package == package)
                                .is_some_and(|e| e.system),
                            c.running.contains(package),
                        )
                    })
                    .unwrap_or((false, false));
                actions.details_requested = Some((package.to_string(), system, running));
            }
            return;
        };

        ui.label(egui::RichText::new(info.display_name()).strong().size(15.0));
        ui.label(
            egui::RichText::new(format!(
                "v{}  ·  {}",
                info.version_name.as_deref().unwrap_or("—"),
                info.state_label()
            ))
            .color(palette::TEXT_DIM),
        );
        ui.add_space(4.0);

        components::segmented(
            ui,
            &[
                (AppDetailTab::Overview, "Overview"),
                (AppDetailTab::Permissions, "Permissions"),
                (AppDetailTab::Activities, "Activities"),
                (AppDetailTab::Services, "Services"),
                (AppDetailTab::Receivers, "Receivers"),
                (AppDetailTab::Providers, "Providers"),
            ],
            &mut state.apps_view.detail_tab,
        );
        ui.separator();

        match state.apps_view.detail_tab {
            AppDetailTab::Overview => overview_tab(ui, &info),
            AppDetailTab::Permissions => permissions_tab(ui, &info),
            AppDetailTab::Activities => components_tab(ui, "Activities", &info.activities),
            AppDetailTab::Services => components_tab(ui, "Services", &info.services),
            AppDetailTab::Receivers => components_tab(ui, "Receivers", &info.receivers),
            AppDetailTab::Providers => components_tab(ui, "Providers", &info.providers),
        }

        ui.add_space(6.0);
        components::section_title(ui, "ACTIONS");
        actions_row(ui, state, &info, actions);

        // Confirmation dialog for destructive actions.
        if let Some(pending) = state.apps_view.confirm.clone() {
            confirm_app_action(ui, state, &pending, actions);
        }
    });
}

fn confirm_app_action(
    ui: &mut egui::Ui,
    state: &mut AppState,
    pending: &PendingAppAction,
    actions: &mut AppsActions,
) {
    let title = format!("Confirm {}", pending.kind.label());
    let what = format!(
        "Are you sure you want to {}:",
        pending.kind.label().to_lowercase()
    );
    let mut lines: Vec<(&str, bool)> = vec![(&what, true), (&pending.package, false)];
    if !pending.label.is_empty() && pending.label != pending.package {
        lines.push((&pending.label, false));
    }
    if pending.kind == AppActionKind::ClearData {
        lines.push((
            "This deletes all app data (accounts, settings, files).",
            false,
        ));
    }
    lines.push(("This action cannot be undone.", false));
    match components::confirm_modal(
        ui.ctx(),
        "app-confirm",
        &title,
        &lines,
        pending.kind.label(),
        true,
    ) {
        Some(true) => {
            actions.action_requested = Some((pending.package.clone(), pending.kind));
            state.apps_view.confirm = None;
        }
        Some(false) => {
            state.apps_view.confirm = None;
        }
        None => {}
    }
}

fn overview_tab(ui: &mut egui::Ui, info: &AppInfo) {
    if info.partial {
        components::warning_line(ui, "Only partial details could be read for this app.");
    }
    components::kv_grid(
        ui,
        "app-overview",
        &[
            ("Application", info.label.as_deref().unwrap_or("—")),
            ("Package", &info.package),
            ("Version", info.version_name.as_deref().unwrap_or("—")),
            ("Version code", info.version_code.as_deref().unwrap_or("—")),
            ("UID", info.uid.as_deref().unwrap_or("—")),
            ("Installer", info.installer.as_deref().unwrap_or("—")),
            ("Install type", if info.system { "System" } else { "User" }),
            ("State", info.state_label()),
        ],
    );
    if !info.apk_paths.is_empty() {
        ui.add_space(4.0);
        components::section_title(ui, "APK PATHS ON DEVICE");
        for p in &info.apk_paths {
            ui.monospace(p);
        }
    }
}

fn permissions_tab(ui: &mut egui::Ui, info: &AppInfo) {
    if info.install_permissions.is_empty() && info.runtime_permissions.is_empty() {
        ui.label(
            egui::RichText::new(
                "No permissions reported. Older Android versions may not expose them via dumpsys.",
            )
            .color(palette::TEXT_DIM),
        );
        return;
    }
    if !info.runtime_permissions.is_empty() {
        components::section_title(ui, "RUNTIME PERMISSIONS");
        permission_list(ui, &info.runtime_permissions);
    }
    if !info.install_permissions.is_empty() {
        components::section_title(ui, "INSTALL PERMISSIONS");
        permission_list(ui, &info.install_permissions);
    }
}

fn permission_list(ui: &mut egui::Ui, perms: &[crate::apps::PermissionStatus]) {
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .show(ui, |ui| {
            for p in perms {
                ui.horizontal(|ui| {
                    if p.granted {
                        components::pill(ui, "GRANTED", palette::SUCCESS, palette::SUCCESS_TINT);
                    } else {
                        components::pill(ui, "DENIED", palette::TEXT_DIM, palette::PANEL);
                    }
                    ui.monospace(&p.name);
                });
            }
        });
}

fn components_tab(ui: &mut egui::Ui, title: &str, items: &[String]) {
    if items.is_empty() {
        ui.label(
            egui::RichText::new(format!(
                "No {title} reported. (Some devices omit resolver tables.)"
            ))
            .color(palette::TEXT_DIM),
        );
        return;
    }
    ui.label(
        egui::RichText::new(format!("{} {}", items.len(), title.to_lowercase()))
            .small()
            .color(palette::TEXT_DIM),
    );
    egui::ScrollArea::vertical()
        .max_height(340.0)
        .show(ui, |ui| {
            for item in items {
                ui.monospace(item);
            }
        });
}

fn actions_row(ui: &mut egui::Ui, state: &mut AppState, info: &AppInfo, actions: &mut AppsActions) {
    let busy = state.apps_view.busy.is_some();
    ui.horizontal_wrapped(|ui| {
        if busy {
            ui.disable();
        }
        for kind in [
            AppActionKind::Launch,
            AppActionKind::ForceStop,
            AppActionKind::ClearCache,
            AppActionKind::ClearData,
            AppActionKind::Extract,
            AppActionKind::Uninstall,
        ] {
            // Uninstalling system apps via pm always fails; disable with a
            // tooltip instead of running into a guaranteed failure.
            if kind == AppActionKind::Uninstall && info.system {
                ui.add_enabled(false, egui::Button::new(kind.label()))
                    .on_hover_text("System apps cannot be uninstalled via ADB.");
                continue;
            }
            let dangerous = kind.needs_confirm();
            let clicked = if dangerous {
                components::danger_button(ui, kind.label()).clicked()
            } else {
                components::secondary_button(ui, kind.label()).clicked()
            };
            if clicked {
                if dangerous {
                    state.apps_view.confirm = Some(PendingAppAction {
                        kind,
                        package: info.package.clone(),
                        label: info.display_name(),
                    });
                } else {
                    actions.action_requested = Some((info.package.clone(), kind));
                }
            }
        }
    });
    if let Some(tag) = state.apps_view.busy.clone() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("Working… ({tag})"));
        });
    }
}
