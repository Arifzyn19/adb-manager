//! App Manager page (Phase 4): installed apps, filters, search, details,
//! actions (launch / force-stop / clear / uninstall / extract APK).

use crate::apps::{AppActionKind, AppDetailTab, AppFilter, AppInfo};
use crate::state::{AppState, PendingAppAction};
use crate::ui::theme::{mono, StatusColors};

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
        ui.heading("Apps");
        ui.add_space(6.0);
        ui.label("No Android device connected.");
        ui.label("Connect a device using USB or Wireless ADB.");
        if ui.button("Connect Device").clicked() {
            state.show_connect_dialog = true;
        }
        return actions;
    };
    if !device.state.is_usable() {
        ui.heading("Apps");
        ui.label(format!(
            "App list unavailable while the device is '{}'.",
            device.state.label()
        ));
        return actions;
    }

    // Detail view takes over the page when a package is selected.
    if let Some(pkg) = state.apps_view.selected.clone() {
        show_details(ui, state, &device.serial, &pkg, &mut actions);
        return actions;
    }

    show_list(ui, state, &device.serial, &mut actions);
    actions
}

// --- List ------------------------------------------------------------------

fn show_list(ui: &mut egui::Ui, state: &mut AppState, serial: &str, actions: &mut AppsActions) {
    ui.horizontal(|ui| {
        ui.heading("Apps");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Refresh").clicked() {
                actions.refresh_requested = true;
            }
        });
    });

    // Filter tabs.
    ui.horizontal(|ui| {
        for filter in [
            AppFilter::All,
            AppFilter::User,
            AppFilter::System,
            AppFilter::Running,
        ] {
            if ui
                .selectable_label(state.apps_view.filter == filter, filter.label())
                .clicked()
            {
                state.apps_view.filter = filter;
            }
        }
    });

    // Search.
    ui.horizontal(|ui| {
        ui.label("Search");
        ui.text_edit_singleline(&mut state.apps_view.search);
    });

    // Loading / progress.
    let (loading, resolving, progress) = state
        .apps
        .get(serial)
        .map(|c| (c.list_loading, c.resolving, c.progress))
        .unwrap_or((true, false, (0, 0)));
    if loading && state.apps.get(serial).is_none_or(|c| c.entries.is_empty()) {
        ui.add_space(8.0);
        ui.label("Loading installed apps… (pm list packages)");
        return;
    }
    if resolving {
        ui.label(format!(
            "Resolving details… {}/{} (labels, versions)",
            progress.0, progress.1
        ));
    }

    let query = state.apps_view.search.to_lowercase();
    let filter = state.apps_view.filter;
    let cache = state.apps.get(serial);
    let mut rows: Vec<(String, bool, Option<AppInfo>)> = Vec::new();
    if let Some(cache) = cache {
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

    if rows.is_empty() {
        ui.add_space(8.0);
        if filter == AppFilter::Running {
            ui.label("No running apps detected.");
            ui.label("The Running tab reads `ps` output; some devices restrict it.");
        } else {
            ui.label("No applications found.");
        }
        return;
    }

    ui.label(format!("{} apps", rows.len()));
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (package, system, info) in &rows {
            app_row(ui, state, package, *system, info.clone(), actions);
        }
    });
}

fn app_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    package: &str,
    system: bool,
    info: Option<AppInfo>,
    actions: &mut AppsActions,
) {
    let (name, version, state_label, state_color) = match &info {
        Some(i) => (
            i.display_name(),
            i.version_name.clone().unwrap_or_else(|| "—".to_string()),
            i.state_label().to_string(),
            match i.state_label() {
                "Running" => StatusColors::connected(),
                "Disabled" => StatusColors::warning(),
                _ => StatusColors::muted(),
            },
        ),
        None => (
            package.to_string(),
            "…".to_string(),
            (if system { "System" } else { "User" }).to_string(),
            StatusColors::muted(),
        ),
    };

    egui::Frame::group(ui.style())
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(name).strong());
                    ui.label(mono(package.to_string()));
                    ui.label(format!("v{version}  •  {state_label}"));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Open").clicked() {
                        state.apps_view.selected = Some(package.to_string());
                        state.apps_view.detail_tab = AppDetailTab::Overview;
                        if info.is_none() {
                            actions.details_requested = Some((package.to_string(), system, false));
                        }
                    }
                    ui.colored_label(state_color, "●");
                });
            });
        });
}

// --- Details ---------------------------------------------------------------

fn show_details(
    ui: &mut egui::Ui,
    state: &mut AppState,
    serial: &str,
    package: &str,
    actions: &mut AppsActions,
) {
    ui.horizontal(|ui| {
        if ui.button("← Back").clicked() {
            state.apps_view.selected = None;
        }
        ui.heading(package);
    });

    let info = state
        .apps
        .get(serial)
        .and_then(|c| c.resolved.get(package))
        .cloned();
    let Some(info) = info else {
        ui.add_space(8.0);
        ui.label("Loading details… (dumpsys package)");
        if ui.button("Retry").clicked() {
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

    ui.label(
        egui::RichText::new(format!(
            "{}  •  v{}  •  {}",
            info.display_name(),
            info.version_name.as_deref().unwrap_or("—"),
            info.state_label()
        ))
        .strong(),
    );
    ui.add_space(4.0);

    // Detail tabs.
    ui.horizontal(|ui| {
        for tab in [
            AppDetailTab::Overview,
            AppDetailTab::Permissions,
            AppDetailTab::Activities,
            AppDetailTab::Services,
            AppDetailTab::Receivers,
            AppDetailTab::Providers,
        ] {
            if ui
                .selectable_label(state.apps_view.detail_tab == tab, tab.label())
                .clicked()
            {
                state.apps_view.detail_tab = tab;
            }
        }
    });
    ui.separator();

    match state.apps_view.detail_tab {
        AppDetailTab::Overview => overview_tab(ui, &info),
        AppDetailTab::Permissions => permissions_tab(ui, &info),
        AppDetailTab::Activities => components_tab(ui, "Activities", &info.activities),
        AppDetailTab::Services => components_tab(ui, "Services", &info.services),
        AppDetailTab::Receivers => components_tab(ui, "Receivers", &info.receivers),
        AppDetailTab::Providers => components_tab(ui, "Providers", &info.providers),
    }

    ui.add_space(8.0);
    ui.strong("Actions");
    actions_row(ui, state, &info, actions);

    // Confirmation dialog for destructive actions.
    if let Some(pending) = state.apps_view.confirm.clone() {
        confirm_dialog(ui, state, &pending, actions);
    }
}

fn overview_tab(ui: &mut egui::Ui, info: &AppInfo) {
    if info.partial {
        ui.colored_label(
            StatusColors::warning(),
            "Only partial details could be read for this app.",
        );
    }
    egui::Grid::new("app-overview")
        .num_columns(2)
        .spacing([12.0, 3.0])
        .striped(true)
        .show(ui, |ui| {
            let rows = [
                ("Application", info.label.as_deref().unwrap_or("—")),
                ("Package", &info.package),
                ("Version", info.version_name.as_deref().unwrap_or("—")),
                ("Version code", info.version_code.as_deref().unwrap_or("—")),
                ("UID", info.uid.as_deref().unwrap_or("—")),
                ("Installer", info.installer.as_deref().unwrap_or("—")),
                (
                    "Installation type",
                    if info.system { "System" } else { "User" },
                ),
                ("State", info.state_label()),
            ];
            for (k, v) in rows {
                ui.label(k);
                ui.monospace(v);
                ui.end_row();
            }
        });
    if !info.apk_paths.is_empty() {
        ui.add_space(4.0);
        ui.strong("APK paths on device");
        for p in &info.apk_paths {
            ui.monospace(p);
        }
    }
}

fn permissions_tab(ui: &mut egui::Ui, info: &AppInfo) {
    if info.install_permissions.is_empty() && info.runtime_permissions.is_empty() {
        ui.label("No permissions reported for this app.");
        ui.label("Older Android versions may not expose them via dumpsys.");
        return;
    }
    if !info.runtime_permissions.is_empty() {
        ui.strong("Runtime permissions");
        permission_list(ui, &info.runtime_permissions);
        ui.add_space(4.0);
    }
    if !info.install_permissions.is_empty() {
        ui.strong("Install permissions");
        permission_list(ui, &info.install_permissions);
    }
}

fn permission_list(ui: &mut egui::Ui, perms: &[crate::apps::PermissionStatus]) {
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .show(ui, |ui| {
            for p in perms {
                ui.horizontal(|ui| {
                    let (dot, color) = if p.granted {
                        ("●", StatusColors::connected())
                    } else {
                        ("○", StatusColors::muted())
                    };
                    ui.colored_label(color, dot);
                    ui.monospace(&p.name);
                    ui.label(if p.granted { "granted" } else { "not granted" });
                });
            }
        });
}

fn components_tab(ui: &mut egui::Ui, title: &str, items: &[String]) {
    if items.is_empty() {
        ui.label(format!(
            "No {title} reported. (Some devices omit resolver tables.)"
        ));
        return;
    }
    ui.label(format!("{} {}", items.len(), title.to_lowercase()));
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
        ui.set_enabled(!busy);
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
                let resp = ui.add_enabled(false, egui::Button::new(kind.label()));
                resp.on_hover_text("System apps cannot be uninstalled via ADB.");
                continue;
            }
            if ui.button(kind.label()).clicked() {
                if kind.needs_confirm() {
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
        ui.label(format!("Working… ({tag})"));
    }
}

fn confirm_dialog(
    ui: &mut egui::Ui,
    state: &mut AppState,
    pending: &PendingAppAction,
    actions: &mut AppsActions,
) {
    egui::Window::new(format!("Confirm {}", pending.kind.label()))
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label(format!(
                "Are you sure you want to {}:",
                pending.kind.label().to_lowercase()
            ));
            ui.monospace(&pending.package);
            if !pending.label.is_empty() && pending.label != pending.package {
                ui.label(&pending.label);
            }
            if pending.kind == AppActionKind::ClearData {
                ui.colored_label(
                    StatusColors::warning(),
                    "This deletes all app data (accounts, settings, files).",
                );
            }
            ui.label("This action cannot be undone.");
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    state.apps_view.confirm = None;
                }
                let destructive =
                    ui.add(egui::Button::new(pending.kind.label()).fill(StatusColors::error()));
                if destructive.clicked() {
                    actions.action_requested = Some((pending.package.clone(), pending.kind));
                    state.apps_view.confirm = None;
                }
            });
        });
}
