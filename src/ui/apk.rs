//! APK Inspector + Installer page: drop zone, segmented metadata tabs,
//! install box. Inspection stays pure-Rust and local; installs run on
//! worker threads.

use crate::apk::inspector::human_size;
use crate::apk::permissions::PermissionLevel;
use crate::state::{ApkTab, AppState};
use crate::ui::components::{self, page_header};
use crate::ui::theme::palette;
use std::path::PathBuf;

pub struct InstallRequest {
    pub serial: String,
    pub files: Vec<String>,
    pub display: String,
    pub reinstall: bool,
}

#[derive(Default)]
pub struct ApkActions {
    pub inspect_path: Option<PathBuf>,
    pub install: Option<InstallRequest>,
}

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut AppState) -> ApkActions {
    let mut actions = ApkActions::default();

    page_header(
        ui,
        "APK Inspector",
        "Local manifest, permission and signature analysis — then install.",
    );

    // Drag & drop anywhere on the page (egui reports hovered + dropped files).
    let dropped: Vec<PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter_map(|f| f.path.clone())
            .collect()
    });
    let hovering = !ctx.input(|i| i.raw.hovered_files.is_empty());
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
        components::loading_state(
            ui,
            "Reading APK",
            "ZIP entries plus binary manifest decode…",
            None,
        );
        return actions;
    }

    let Some(info) = state.apk.info.clone() else {
        drop_zone(ui, state, hovering, &mut actions);
        return actions;
    };

    // Header strip: file + package + version + replace action.
    ui.horizontal(|ui| {
        ui.monospace(&info.file_name);
        ui.label(egui::RichText::new(human_size(info.file_size)).color(palette::TEXT_DIM));
        ui.label(
            egui::RichText::new(&info.manifest.package)
                .color(palette::ACCENT_BRIGHT)
                .strong(),
        );
        if let Some(v) = &info.manifest.version_name {
            ui.monospace(format!("v{v}"));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::secondary_button(ui, "Open another…").clicked() {
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
    if let Some(err) = state.apk.error.clone() {
        components::error_panel(ui, &err, None);
    }

    components::segmented(
        ui,
        &[
            (ApkTab::Overview, "Overview"),
            (ApkTab::Manifest, "Manifest"),
            (ApkTab::Permissions, "Permissions"),
            (ApkTab::Activities, "Activities"),
            (ApkTab::Services, "Services"),
            (ApkTab::Receivers, "Receivers"),
            (ApkTab::Files, "Files"),
            (ApkTab::Certificate, "Certificate"),
        ],
        &mut state.apk.tab,
    );
    ui.separator();

    match state.apk.tab {
        ApkTab::Overview => show_overview(ui, state, &info, &mut actions),
        ApkTab::Manifest => show_manifest(ui, &info),
        ApkTab::Permissions => show_permissions(ui, &info),
        ApkTab::Activities => component_list(ui, "Activities", &info.manifest.activities),
        ApkTab::Services => component_list(ui, "Services", &info.manifest.services),
        ApkTab::Receivers => component_list(ui, "Receivers", &info.manifest.receivers),
        ApkTab::Files => show_files(ui, &info),
        ApkTab::Certificate => show_certificate(ctx, ui, &info),
    }

    actions
}

/// Polished drop zone: dashed-feel bordered panel, big glyph, browse button.
fn drop_zone(ui: &mut egui::Ui, state: &mut AppState, hovering: bool, actions: &mut ApkActions) {
    if let Some(err) = state.apk.error.clone() {
        components::error_panel(ui, &err, None);
        ui.add_space(4.0);
    }
    egui::Frame::new()
        .fill(if hovering {
            palette::ACCENT_TINT
        } else {
            palette::PANEL
        })
        .stroke(egui::Stroke::new(
            1.0,
            if hovering {
                palette::ACCENT_BRIGHT
            } else {
                palette::BORDER_STRONG
            },
        ))
        .corner_radius(6.0.into())
        .inner_margin(28.0.into())
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("⬆").size(30.0).color(if hovering {
                    palette::ACCENT_BRIGHT
                } else {
                    palette::TEXT_FAINT
                }));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Drop an .apk file here")
                        .strong()
                        .size(15.0),
                );
                ui.label(
                    egui::RichText::new("…or pick one from disk. Inspection is 100% local.")
                        .color(palette::TEXT_DIM),
                );
                ui.add_space(10.0);
                if components::primary_button(ui, "Browse for APK…").clicked() {
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
}

fn show_overview(
    ui: &mut egui::Ui,
    state: &mut AppState,
    info: &crate::apk::ApkInfo,
    actions: &mut ApkActions,
) {
    let m = &info.manifest;
    components::kv_grid(
        ui,
        "apk_overview",
        &[
            ("Package", &m.package),
            ("App label", m.app_label.as_deref().unwrap_or("—")),
            ("Version", m.version_name.as_deref().unwrap_or("—")),
            ("Version code", m.version_code.as_deref().unwrap_or("—")),
            ("Min SDK", m.min_sdk.as_deref().unwrap_or("—")),
            ("Target SDK", m.target_sdk.as_deref().unwrap_or("—")),
            ("Compile SDK", m.compile_sdk.as_deref().unwrap_or("—")),
            (
                "File size",
                &format!(
                    "{} ({} uncompressed)",
                    human_size(info.file_size),
                    human_size(info.total_uncompressed)
                ),
            ),
            (
                "Code",
                if info.has_dex {
                    "Dalvik bytecode (.dex)"
                } else {
                    "No .dex (resource-only?)"
                },
            ),
            (
                "Native libs",
                &if info.architectures.is_empty() {
                    "none".to_string()
                } else {
                    info.architectures.join(", ")
                },
            ),
        ],
    );
    // kv_grid borrows &str rows — the temporaries above live long enough
    // because the grid renders synchronously inside this call.
    let dangerous = info
        .permissions
        .iter()
        .filter(|p| p.level == PermissionLevel::Dangerous)
        .count();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Permissions").color(palette::TEXT_DIM));
        ui.monospace(format!(
            "{} total, {dangerous} dangerous",
            info.permissions.len()
        ));
    });
    if m.debuggable {
        components::warning_line(ui, "android:debuggable=true in this build.");
    }

    ui.add_space(6.0);
    components::section_title(ui, "INSTALL ON DEVICE");
    install_box(ui, state, info, m, actions);
}

fn install_box(
    ui: &mut egui::Ui,
    state: &mut AppState,
    info: &crate::apk::ApkInfo,
    m: &crate::apk::manifest::ManifestData,
    actions: &mut ApkActions,
) {
    let device_label = state
        .selected_device()
        .map(|d| format!("{} ({})", d.display_name(), d.serial))
        .unwrap_or_else(|| "No device selected".to_string());
    components::kv_line(ui, "Device", &device_label);
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
    });
    if state.selected_device().is_none() {
        ui.label(egui::RichText::new("Connect a device to install.").color(palette::TEXT_DIM));
    }
}

fn show_permissions(ui: &mut egui::Ui, info: &crate::apk::ApkInfo) {
    if info.permissions.is_empty() {
        ui.label(egui::RichText::new("No permissions declared.").color(palette::TEXT_DIM));
        return;
    }
    ui.label(
        egui::RichText::new(format!(
            "{} permissions (dangerous first)",
            info.permissions.len()
        ))
        .small()
        .color(palette::TEXT_DIM),
    );
    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in &info.permissions {
            let (label, color, tint) = match p.level {
                PermissionLevel::Dangerous => {
                    ("DANGEROUS", palette::WARNING, palette::WARNING_TINT)
                }
                PermissionLevel::Signature => ("SIGNATURE", palette::ERROR, palette::ERROR_TINT),
                PermissionLevel::Normal => ("NORMAL", palette::SUCCESS, palette::SUCCESS_TINT),
                PermissionLevel::Unknown => ("UNKNOWN", palette::TEXT_DIM, palette::PANEL),
            };
            ui.horizontal_wrapped(|ui| {
                components::pill(ui, label, color, tint);
                ui.monospace(&p.name);
            });
            ui.label(
                egui::RichText::new(&p.description)
                    .small()
                    .color(palette::TEXT_DIM),
            );
            ui.separator();
        }
    });
}

fn component_list(ui: &mut egui::Ui, title: &str, items: &[String]) {
    if items.is_empty() {
        ui.label(egui::RichText::new(format!("No {title} declared.")).color(palette::TEXT_DIM));
        return;
    }
    ui.label(
        egui::RichText::new(format!("{} {}", items.len(), title.to_lowercase()))
            .small()
            .color(palette::TEXT_DIM),
    );
    egui::ScrollArea::vertical().show(ui, |ui| {
        for item in items {
            ui.monospace(item);
        }
    });
}

fn show_manifest(ui: &mut egui::Ui, info: &crate::apk::ApkInfo) {
    ui.label(
        egui::RichText::new("Decoded locally from binary AXML — no aapt2, no uploads.")
            .small()
            .color(palette::TEXT_FAINT),
    );
    let m = &info.manifest;
    components::kv_grid(
        ui,
        "apk_manifest",
        &[
            ("package", &m.package),
            ("versionCode", m.version_code.as_deref().unwrap_or("—")),
            ("versionName", m.version_name.as_deref().unwrap_or("—")),
            ("minSdkVersion", m.min_sdk.as_deref().unwrap_or("—")),
            ("targetSdkVersion", m.target_sdk.as_deref().unwrap_or("—")),
            ("compileSdkVersion", m.compile_sdk.as_deref().unwrap_or("—")),
        ],
    );
    ui.add_space(4.0);
    components::section_title(ui, "DECLARED PERMISSIONS (RAW)");
    egui::ScrollArea::vertical()
        .max_height(160.0)
        .show(ui, |ui| {
            for p in &m.permissions {
                ui.monospace(p);
            }
        });
}

fn show_files(ui: &mut egui::Ui, info: &crate::apk::ApkInfo) {
    ui.label(
        egui::RichText::new(format!(
            "{} files, {} uncompressed",
            info.files.len(),
            human_size(info.total_uncompressed)
        ))
        .small()
        .color(palette::TEXT_DIM),
    );
    components::table_header(ui, &[("Size", 80.0), ("Path", 420.0)]);
    let shown: Vec<&crate::apk::ZipEntryInfo> = info.files.iter().take(2000).collect();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for f in shown {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [80.0, 16.0],
                    egui::Label::new(
                        egui::RichText::new(human_size(f.size_bytes))
                            .monospace()
                            .small(),
                    ),
                );
                ui.monospace(&f.name);
            });
        }
        if info.files.len() > 2000 {
            ui.label(
                egui::RichText::new(format!(
                    "…and {} more (list capped)",
                    info.files.len() - 2000
                ))
                .small()
                .color(palette::TEXT_FAINT),
            );
        }
    });
}

fn show_certificate(ctx: &egui::Context, ui: &mut egui::Ui, info: &crate::apk::ApkInfo) {
    let s = &info.signatures;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("JAR (v1)").color(palette::TEXT_DIM));
        ui.monospace(if s.has_v1 {
            "present (META-INF/*.SF)"
        } else {
            "absent"
        });
        ui.label(egui::RichText::new("Signing block (v2/v3)").color(palette::TEXT_DIM));
        ui.monospace(if s.has_v2_block {
            "detected"
        } else {
            "not found"
        });
    });
    if s.certs.is_empty() {
        ui.add_space(4.0);
        components::warning_line(
            ui,
            "No signer certificates in META-INF — unsigned or unrecognized scheme.",
        );
        return;
    }
    for cert in &s.certs {
        ui.separator();
        ui.monospace(&cert.file);
        components::kv_grid(
            ui,
            &format!("cert_{}", cert.file),
            &[
                ("SHA-256", &cert.sha256),
                ("Size", &human_size(cert.size_bytes)),
                ("Subject", cert.subject.as_deref().unwrap_or("—")),
                ("Issuer", cert.issuer.as_deref().unwrap_or("—")),
                ("Validity", cert.validity.as_deref().unwrap_or("—")),
            ],
        );
        if ui.small_button("⧉ Copy SHA-256").clicked() {
            ctx.copy_text(cert.sha256.clone());
        }
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Subject / issuer / validity are best-effort DER scans; fingerprints are exact SHA-256.",
        )
        .small()
        .color(palette::TEXT_FAINT),
    );
}

/// Sibling splits for a base APK: `split_config.*.apk` next to the selected
/// file. Returns just the selected path when no siblings exist.
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
