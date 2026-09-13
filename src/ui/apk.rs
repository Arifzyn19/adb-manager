//! APK Inspector + Installer page (Phase 7).
//!
//! Pure-Rust inspection (ZIP + binary-AXML decode + signature summary) runs
//! on a worker thread; `adb install [-r]` / `install-multiple [-r]` runs on
//! a worker thread too. The UI thread only renders state and returns actions.

use crate::apk::inspector::human_size;
use crate::apk::permissions::PermissionLevel;
use crate::state::{ApkTab, AppState};
use crate::ui::theme::StatusColors;
use std::path::PathBuf;

pub struct InstallRequest {
    pub serial: String,
    pub files: Vec<String>,
    pub display: String,
    pub reinstall: bool,
}

pub struct ApkActions {
    pub inspect_path: Option<PathBuf>,
    pub install: Option<InstallRequest>,
}

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut AppState) -> ApkActions {
    let mut actions = ApkActions {
        inspect_path: None,
        install: None,
    };

    ui.horizontal(|ui| {
        ui.heading("APK Inspector");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Open APK…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Android package", &["apk"])
                    .set_title("Select APK file")
                    .pick_file()
                {
                    actions.inspect_path = Some(path);
                }
            }
        });
    });

    // Drag & drop anywhere on the page (egui reports hovered + dropped files).
    let dropped: Vec<PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter_map(|f| f.path.clone())
            .collect()
    });
    if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
        ui.label("Drop the .apk file to inspect it.");
    }
    // One file at a time by design.
    if let Some(path) = dropped.into_iter().next() {
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("apk"))
        {
            actions.inspect_path = Some(path);
        } else {
            state.apk.error = Some(format!("{} is not an .apk file.", path.display()));
        }
    }

    if state.apk.loading {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Reading APK… (ZIP + manifest decode)");
        });
        return actions;
    }

    let Some(info) = state.apk.info.clone() else {
        ui.add_space(6.0);
        if let Some(err) = state.apk.error.clone() {
            ui.colored_label(StatusColors::error(), format!("✕ {err}"));
            ui.add_space(4.0);
        }
        ui.label("No APK loaded.");
        ui.label("Open a .apk file (or drop it here) to inspect its manifest, permissions, components, files and signatures — then install it on the selected device.");
        if state.apk.path.is_none() && state.apk.error.is_none() {
            ui.add_space(4.0);
            ui.colored_label(
                StatusColors::muted(),
                "Nothing ever leaves your machine: inspection is local ZIP/XML parsing.",
            );
        }
        return actions;
    };

    // Header strip: file + package + version.
    ui.horizontal_wrapped(|ui| {
        ui.monospace(&info.file_name);
        ui.colored_label(StatusColors::muted(), human_size(info.file_size));
        ui.colored_label(StatusColors::accent(), &info.manifest.package);
        if let Some(v) = &info.manifest.version_name {
            ui.monospace(format!("v{v}"));
        }
    });

    // Tabs.
    ui.horizontal_wrapped(|ui| {
        for tab in [
            ApkTab::Overview,
            ApkTab::Permissions,
            ApkTab::Components,
            ApkTab::Manifest,
            ApkTab::Files,
            ApkTab::Certificate,
        ] {
            let active = state.apk.tab == tab;
            if ui.selectable_label(active, tab.label()).clicked() {
                state.apk.tab = tab;
            }
        }
    });
    ui.separator();

    match state.apk.tab {
        ApkTab::Overview => show_overview(ui, state, &info, &mut actions),
        ApkTab::Permissions => show_permissions(ui, &info),
        ApkTab::Components => show_components(ui, &info),
        ApkTab::Manifest => show_manifest(ui, &info),
        ApkTab::Files => show_files(ui, &info),
        ApkTab::Certificate => show_certificate(ctx, ui, &info),
    }

    actions
}

fn show_overview(
    ui: &mut egui::Ui,
    state: &mut AppState,
    info: &crate::apk::ApkInfo,
    actions: &mut ApkActions,
) {
    let m = &info.manifest;
    egui::Grid::new("apk_overview")
        .num_columns(2)
        .show(ui, |ui| {
            kv(ui, "Package", &m.package);
            kv_opt(ui, "App label", m.app_label.as_deref());
            kv_opt(ui, "Version", m.version_name.as_deref());
            kv_opt(ui, "Version code", m.version_code.as_deref());
            kv_opt(ui, "Min SDK", m.min_sdk.as_deref());
            kv_opt(ui, "Target SDK", m.target_sdk.as_deref());
            kv_opt(ui, "Compile SDK", m.compile_sdk.as_deref());
            ui.label("File size");
            ui.monospace(format!(
                "{} ({} uncompressed)",
                human_size(info.file_size),
                human_size(info.total_uncompressed)
            ));
            ui.end_row();
            ui.label("Code");
            ui.monospace(if info.has_dex {
                "Dalvik bytecode present (.dex)".to_string()
            } else {
                "No .dex found (resource-only?)".to_string()
            });
            ui.end_row();
            ui.label("Native libs");
            ui.monospace(if info.architectures.is_empty() {
                "none".to_string()
            } else {
                info.architectures.join(", ")
            });
            ui.end_row();
            let dangerous = info
                .permissions
                .iter()
                .filter(|p| p.level == PermissionLevel::Dangerous)
                .count();
            ui.label("Permissions");
            ui.monospace(format!(
                "{} total, {dangerous} dangerous",
                info.permissions.len()
            ));
            ui.end_row();
            if m.debuggable {
                ui.label("Debuggable");
                ui.colored_label(StatusColors::warning(), "⚠ android:debuggable=true");
                ui.end_row();
            }
        });

    ui.add_space(8.0);
    ui.strong("Install on device");
    let device_label = state
        .selected_device()
        .map(|d| format!("{} ({})", d.display_name(), d.serial))
        .unwrap_or_else(|| "No device selected".to_string());
    ui.horizontal(|ui| {
        ui.label("Device");
        ui.monospace(&device_label);
    });
    ui.checkbox(
        &mut state.apk.reinstall,
        "Reinstall (-r): replace existing app, keep its data",
    );
    if let Some(last) = state.apk.last_install.clone() {
        ui.monospace(&last);
    }
    ui.horizontal(|ui| {
        let can_install = !state.apk.installing && state.selected_device().is_some();
        if ui
            .add_enabled(can_install, egui::Button::new("Install APK"))
            .clicked()
        {
            if let (Some(serial), Some(path)) =
                (state.selected_serial.clone(), state.apk.path.clone())
            {
                // Split bundles: offer sibling split_config.*.apk files next to
                // base.apk automatically (install-multiple path).
                let files = split_siblings(&path);
                actions.install = Some(InstallRequest {
                    serial,
                    files,
                    display: if m.package.is_empty() {
                        info.file_name.clone()
                    } else {
                        m.package.clone()
                    },
                    reinstall: state.apk.reinstall,
                });
            }
        }
        if state.apk.installing {
            ui.spinner();
            ui.label(format!(
                "Installing on {}… (large APKs take minutes)",
                state.selected_serial.clone().unwrap_or_default()
            ));
        }
        if state.selected_device().is_none() {
            ui.colored_label(StatusColors::muted(), "Connect a device to install.");
        }
    });
}

fn show_permissions(ui: &mut egui::Ui, info: &crate::apk::ApkInfo) {
    if info.permissions.is_empty() {
        ui.label("No permissions declared.");
        return;
    }
    ui.label(format!(
        "{} permissions (dangerous first)",
        info.permissions.len()
    ));
    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in &info.permissions {
            let (icon, color) = match p.level {
                PermissionLevel::Dangerous => ("● Dangerous", StatusColors::warning()),
                PermissionLevel::Signature => ("● Signature", StatusColors::error()),
                PermissionLevel::Normal => ("● Normal", StatusColors::connected()),
                PermissionLevel::Unknown => ("● Unknown", StatusColors::muted()),
            };
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(color, icon);
                ui.monospace(&p.name);
            });
            ui.colored_label(StatusColors::muted(), &p.description);
            ui.separator();
        }
    });
}

fn show_components(ui: &mut egui::Ui, info: &crate::apk::ApkInfo) {
    let m = &info.manifest;
    component_group(ui, "Activities", &m.activities);
    component_group(ui, "Services", &m.services);
    component_group(ui, "Receivers", &m.receivers);
    component_group(ui, "Providers", &m.providers);
    if m.features.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.strong("Required features");
    for f in &m.features {
        ui.monospace(f);
    }
}

fn component_group(ui: &mut egui::Ui, title: &str, items: &[String]) {
    ui.strong(format!("{title} ({})", items.len()));
    if items.is_empty() {
        ui.colored_label(StatusColors::muted(), "none declared");
    } else {
        egui::ScrollArea::vertical()
            .max_height(140.0)
            .show(ui, |ui| {
                for item in items {
                    ui.monospace(item);
                }
            });
    }
    ui.add_space(4.0);
}

fn show_manifest(ui: &mut egui::Ui, info: &crate::apk::ApkInfo) {
    ui.colored_label(
        StatusColors::muted(),
        "Decoded locally from binary AXML — no aapt2, no uploads.",
    );
    let m = &info.manifest;
    egui::Grid::new("apk_manifest")
        .num_columns(2)
        .show(ui, |ui| {
            kv(ui, "package", &m.package);
            kv_opt(ui, "versionCode", m.version_code.as_deref());
            kv_opt(ui, "versionName", m.version_name.as_deref());
            kv_opt(ui, "minSdkVersion", m.min_sdk.as_deref());
            kv_opt(ui, "targetSdkVersion", m.target_sdk.as_deref());
            kv_opt(ui, "compileSdkVersion", m.compile_sdk.as_deref());
        });
    ui.add_space(4.0);
    ui.strong("Raw permission names (as declared)");
    egui::ScrollArea::vertical()
        .max_height(160.0)
        .show(ui, |ui| {
            for p in &m.permissions {
                ui.monospace(p);
            }
        });
}

fn show_files(ui: &mut egui::Ui, info: &crate::apk::ApkInfo) {
    ui.label(format!(
        "{} files, {} uncompressed",
        info.files.len(),
        human_size(info.total_uncompressed)
    ));
    let shown: Vec<&crate::apk::ZipEntryInfo> = info.files.iter().take(2000).collect();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for f in shown {
            ui.horizontal(|ui| {
                ui.monospace(human_size(f.size_bytes));
                ui.monospace(&f.name);
            });
        }
        if info.files.len() > 2000 {
            ui.colored_label(
                StatusColors::muted(),
                format!("…and {} more (list capped)", info.files.len() - 2000),
            );
        }
    });
}

fn show_certificate(ctx: &egui::Context, ui: &mut egui::Ui, info: &crate::apk::ApkInfo) {
    let s = &info.signatures;
    ui.horizontal(|ui| {
        ui.label("JAR (v1)");
        ui.monospace(if s.has_v1 {
            "present (META-INF/*.SF)"
        } else {
            "absent"
        });
        ui.label("APK Signing Block (v2/v3)");
        ui.monospace(if s.has_v2_block {
            "detected"
        } else {
            "not found"
        });
    });
    if s.certs.is_empty() {
        ui.add_space(4.0);
        ui.colored_label(
            StatusColors::warning(),
            "No signer certificates found in META-INF. The APK may be unsigned or use an unrecognized scheme.",
        );
        return;
    }
    for cert in &s.certs {
        ui.separator();
        ui.monospace(&cert.file);
        egui::Grid::new(format!("cert_{}", cert.file))
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("SHA-256");
                ui.horizontal(|ui| {
                    ui.monospace(&cert.sha256);
                    if ui.small_button("⧉").clicked() {
                        ctx.copy_text(cert.sha256.clone());
                    }
                });
                ui.end_row();
                ui.label("Size");
                ui.monospace(human_size(cert.size_bytes));
                ui.end_row();
                if let Some(sub) = &cert.subject {
                    ui.label("Subject");
                    ui.monospace(sub);
                    ui.end_row();
                }
                if let Some(iss) = &cert.issuer {
                    ui.label("Issuer");
                    ui.monospace(iss);
                    ui.end_row();
                }
                if let Some(v) = &cert.validity {
                    ui.label("Validity");
                    ui.monospace(v);
                    ui.end_row();
                }
            });
    }
    ui.add_space(4.0);
    ui.colored_label(
        StatusColors::muted(),
        "Subject / issuer / validity are best-effort DER scans; fingerprints are exact SHA-256 over the signature block.",
    );
}

fn kv(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.label(key);
    ui.monospace(value);
    ui.end_row();
}

fn kv_opt(ui: &mut egui::Ui, key: &str, value: Option<&str>) {
    ui.label(key);
    match value {
        Some(v) => {
            ui.monospace(v);
        }
        None => {
            ui.colored_label(StatusColors::muted(), "—");
        }
    }
    ui.end_row();
}

/// Sibling splits for a base APK: `<stem>/split_config.*.apk` next to the
/// selected file (AAB-installed apps pulled as multiple files). Returns just
/// the selected path when no siblings exist — plain `adb install` path.
fn split_siblings(selected: &str) -> Vec<String> {
    let path = std::path::Path::new(selected);
    let Some(parent) = path.parent() else {
        return vec![selected.to_string()];
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return vec![selected.to_string()];
    };
    let mut splits: Vec<String> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case("apk"))
                && p.to_string_lossy() != selected
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("split_"))
        })
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    if splits.is_empty() {
        return vec![selected.to_string()];
    }
    splits.sort();
    let mut out = vec![selected.to_string()];
    out.append(&mut splits);
    out
}
