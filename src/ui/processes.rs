//! Process Manager page (Phase 6): sortable/searchable process table with
//! refresh, force-stop/kill actions and clipboard helpers.

use crate::processes::{ProcessInfo, SortColumn};
use crate::state::AppState;
use crate::ui::theme::{mono, StatusColors};

pub enum ProcAction {
    ForceStop(String),
    Kill(u32),
}

#[derive(Default)]
pub struct ProcActions {
    pub refresh_requested: bool,
    pub action_requested: Option<ProcAction>,
}

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut AppState) -> ProcActions {
    let mut actions = ProcActions::default();

    ui.horizontal(|ui| {
        ui.heading("Processes");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Refresh").clicked() {
                actions.refresh_requested = true;
            }
            ui.checkbox(&mut state.proc_view.auto_refresh, "Auto refresh");
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
            "Process list unavailable while the device is '{}'.",
            device.state.label()
        ));
        return actions;
    }
    let serial = device.serial.clone();

    ui.horizontal(|ui| {
        ui.label("Search");
        ui.text_edit_singleline(&mut state.proc_view.search);
    });
    ui.label("CPU and memory are live snapshots — approximate, not exact.");

    let loading = state.processes.get(&serial).is_some_and(|c| c.loading);
    if loading
        && state
            .processes
            .get(&serial)
            .is_none_or(|c| c.procs.is_empty())
    {
        ui.add_space(8.0);
        ui.label("Reading process list… (ps)");
        return actions;
    }

    let query = state.proc_view.search.to_lowercase();
    let (sort, ascending) = (state.proc_view.sort, state.proc_view.ascending);
    let mut rows: Vec<ProcessInfo> = state
        .processes
        .get(&serial)
        .map(|c| c.procs.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| {
            query.is_empty()
                || p.name.to_lowercase().contains(&query)
                || p.pid.to_string().contains(&query)
        })
        .collect();
    rows.sort_by(|a, b| {
        let ord = match sort {
            SortColumn::Pid => a.pid.cmp(&b.pid),
            SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortColumn::Cpu => a
                .cpu_pct
                .unwrap_or(-1.0)
                .partial_cmp(&b.cpu_pct.unwrap_or(-1.0))
                .unwrap_or(std::cmp::Ordering::Equal),
            SortColumn::Memory => a.rss_kb.cmp(&b.rss_kb),
        };
        if ascending {
            ord
        } else {
            ord.reverse()
        }
    });

    if rows.is_empty() {
        ui.add_space(8.0);
        ui.label("No processes match the current search.");
        return actions;
    }

    ui.label(format!("{} processes", rows.len()));

    // Header with sort controls.
    ui.horizontal(|ui| {
        sort_header(ui, state, SortColumn::Pid, 70.0);
        sort_header(ui, state, SortColumn::Name, 220.0);
        ui.label("Package");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label("Actions");
            sort_header(ui, state, SortColumn::Memory, 80.0);
            sort_header(ui, state, SortColumn::Cpu, 70.0);
        });
    });
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in &rows {
            proc_row(ctx, ui, state, p, &mut actions);
        }
    });

    if let Some(tag) = state.proc_view.busy.clone() {
        ui.separator();
        ui.label(format!("Working… ({tag})"));
    }

    actions
}

fn sort_header(ui: &mut egui::Ui, state: &mut AppState, col: SortColumn, width: f32) {
    let active = state.proc_view.sort == col;
    let arrow = if active {
        if state.proc_view.ascending {
            " ▲"
        } else {
            " ▼"
        }
    } else {
        ""
    };
    let label = format!("{}{}", col.label(), arrow);
    if ui
        .add_sized([width, 20.0], egui::Button::new(label))
        .clicked()
    {
        if active {
            state.proc_view.ascending = !state.proc_view.ascending;
        } else {
            state.proc_view.sort = col;
            state.proc_view.ascending = col == SortColumn::Name;
        }
    }
}

fn proc_row(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    state: &mut AppState,
    p: &ProcessInfo,
    actions: &mut ProcActions,
) {
    let busy = state.proc_view.busy.is_some();
    ui.horizontal(|ui| {
        ui.set_enabled(!busy);
        ui.add_sized([70.0, 18.0], egui::Label::new(mono(p.pid.to_string())));
        ui.add_sized(
            [220.0, 18.0],
            egui::Label::new(mono(p.name.clone())).truncate(),
        );
        match p.package_guess() {
            Some(pkg) => {
                ui.monospace(pkg);
            }
            None => {
                ui.colored_label(StatusColors::muted(), "—");
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("⧉ PKG").clicked() {
                if let Some(pkg) = p.package_guess() {
                    ctx.copy_text(pkg.to_string());
                }
            }
            if ui.small_button("⧉ PID").clicked() {
                ctx.copy_text(p.pid.to_string());
            }
            if ui.small_button("Kill").clicked() {
                actions.action_requested = Some(ProcAction::Kill(p.pid));
            }
            match p.package_guess() {
                Some(pkg) => {
                    if ui.small_button("Stop").clicked() {
                        actions.action_requested = Some(ProcAction::ForceStop(pkg.to_string()));
                    }
                }
                None => {
                    let resp = ui.add_enabled(false, egui::Button::new("Stop"));
                    resp.on_hover_text("Only app processes can be force-stopped.");
                }
            }
            ui.monospace(p.rss_display());
            ui.monospace(p.cpu_display());
        });
    });
}
