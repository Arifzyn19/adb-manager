//! Process Manager page: sortable/searchable table with state column,
//! context menu, refresh controls and kill/force-stop actions.

use crate::processes::{ProcessInfo, SortColumn};
use crate::state::AppState;
use crate::ui::components::{self, page_header};
use crate::ui::theme::{mono, palette};

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

    page_header(
        ui,
        "Processes",
        "Live process snapshots — approximate, not exact.",
    );

    let Some(device) = state.selected_device().cloned() else {
        components::no_device_state(ui, state);
        return actions;
    };
    if !device.state.is_usable() {
        ui.label(
            egui::RichText::new(format!(
                "Process list unavailable while the device is '{}'.",
                device.state.label()
            ))
            .color(palette::TEXT_DIM),
        );
        return actions;
    }
    let serial = device.serial.clone();

    ui.horizontal(|ui| {
        if components::secondary_button(ui, "Refresh").clicked() {
            actions.refresh_requested = true;
        }
        ui.checkbox(&mut state.proc_view.auto_refresh, "Auto refresh (5s)");
    });

    components::search_field(
        ui,
        &mut state.proc_view.search,
        "Search process, package or PID…",
    );
    ui.add_space(4.0);

    let loading = state.processes.get(&serial).is_some_and(|c| c.loading);
    if loading
        && state
            .processes
            .get(&serial)
            .is_none_or(|c| c.procs.is_empty())
    {
        components::loading_state(
            ui,
            "Reading process list",
            "ps -A plus top CPU snapshot…",
            None,
        );
        return actions;
    }

    let rows = filtered_rows(state, &serial);
    if rows.is_empty() {
        ui.label(
            egui::RichText::new("No processes match the current search.").color(palette::TEXT_DIM),
        );
        return actions;
    }

    ui.label(
        egui::RichText::new(format!("{} processes", rows.len()))
            .small()
            .color(palette::TEXT_DIM),
    );
    components::table_header(
        ui,
        &[
            ("PID", 64.0),
            ("Process", 210.0),
            ("Package", 200.0),
            ("CPU", 60.0),
            ("Memory", 80.0),
            ("State", 90.0),
        ],
    );

    // Clickable sort bar (mirrors the column order above).
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Sort:")
                .small()
                .color(palette::TEXT_FAINT),
        );
        for col in [
            SortColumn::Pid,
            SortColumn::Name,
            SortColumn::Cpu,
            SortColumn::Memory,
        ] {
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
            if ui
                .selectable_label(active, format!("{}{}", col.label(), arrow))
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
    });

    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in &rows {
            proc_row(ctx, ui, state, p, &mut actions);
        }
    });

    if let Some(tag) = state.proc_view.busy.clone() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("Working… ({tag})"));
        });
    }

    actions
}

fn filtered_rows(state: &AppState, serial: &str) -> Vec<ProcessInfo> {
    let query = state.proc_view.search.to_lowercase();
    let (sort, ascending) = (state.proc_view.sort, state.proc_view.ascending);
    let mut rows: Vec<ProcessInfo> = state
        .processes
        .get(serial)
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
    rows
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
        if busy {
            ui.disable();
        }
        ui.add_sized([64.0, 18.0], egui::Label::new(mono(p.pid.to_string())));
        let name_resp = ui.add_sized(
            [210.0, 18.0],
            egui::Label::new(mono(p.name.clone())).truncate(),
        );
        match p.package_guess() {
            Some(pkg) => {
                ui.add_sized([200.0, 18.0], egui::Label::new(mono(pkg)).truncate());
            }
            None => {
                ui.add_sized(
                    [200.0, 18.0],
                    egui::Label::new(egui::RichText::new("—").color(palette::TEXT_FAINT)),
                );
            }
        }
        ui.add_sized([60.0, 18.0], egui::Label::new(mono(p.cpu_display())));
        ui.add_sized([80.0, 18.0], egui::Label::new(mono(p.rss_display())));
        ui.add_sized(
            [90.0, 18.0],
            egui::Label::new(
                egui::RichText::new("Running")
                    .small()
                    .color(palette::SUCCESS),
            ),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::secondary_button(ui, "Kill").clicked() {
                actions.action_requested = Some(ProcAction::Kill(p.pid));
            }
            match p.package_guess() {
                Some(pkg) => {
                    if components::secondary_button(ui, "Stop").clicked() {
                        actions.action_requested = Some(ProcAction::ForceStop(pkg.to_string()));
                    }
                }
                None => {
                    ui.add_enabled(false, egui::Button::new("Stop"))
                        .on_hover_text("Only app processes can be force-stopped.");
                }
            }
        });

        // Right-click context menu mirrors the buttons + clipboard helpers.
        name_resp.context_menu(|ui| {
            if ui.button("Copy PID").clicked() {
                ctx.copy_text(p.pid.to_string());
                ui.close();
            }
            if let Some(pkg) = p.package_guess() {
                if ui.button("Copy package").clicked() {
                    ctx.copy_text(pkg.to_string());
                    ui.close();
                }
                if ui.button("Force stop package").clicked() {
                    actions.action_requested = Some(ProcAction::ForceStop(pkg.to_string()));
                    ui.close();
                }
            }
            if ui.button("Kill process").clicked() {
                actions.action_requested = Some(ProcAction::Kill(p.pid));
                ui.close();
            }
        });
    });
}
