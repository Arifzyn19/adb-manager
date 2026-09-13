//! Left navigation sidebar.

use crate::state::Page;
use egui::Ui;

const NAV_TOP: &[(Page, &str)] = &[(Page::Dashboard, "Dashboard"), (Page::Devices, "Devices")];
const NAV_MANAGE: &[(Page, &str)] = &[
    (Page::Apps, "Apps"),
    (Page::Processes, "Processes"),
    (Page::Files, "Files"),
];
const NAV_DEBUG: &[(Page, &str)] = &[
    (Page::Logcat, "Logcat"),
    (Page::Apk, "APK"),
    (Page::Shell, "Shell"),
];
const NAV_TOOLS: &[(Page, &str)] = &[(Page::Tools, "Device Tools"), (Page::Settings, "Settings")];

pub fn show(ui: &mut Ui, current: &mut Page) {
    ui.heading("ADB Manager");
    ui.add_space(4.0);
    ui.separator();

    for (page, label) in NAV_TOP {
        nav_item(ui, current, *page, label);
    }

    ui.add_space(4.0);
    ui.strong("MANAGEMENT");
    for (page, label) in NAV_MANAGE {
        nav_item(ui, current, *page, label);
    }

    ui.add_space(4.0);
    ui.strong("DEBUG");
    for (page, label) in NAV_DEBUG {
        nav_item(ui, current, *page, label);
    }

    ui.add_space(4.0);
    ui.strong("TOOLS");
    for (page, label) in NAV_TOOLS {
        nav_item(ui, current, *page, label);
    }
}

fn nav_item(ui: &mut Ui, current: &mut Page, page: Page, label: &str) {
    let selected = *current == page;
    if ui.selectable_label(selected, label).clicked() {
        *current = page;
    }
}
