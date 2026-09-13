//! Compact top header: device selector + settings shortcut.

use crate::state::{AppState, Page};
use crate::ui::theme::palette;

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    egui::TopBottomPanel::top("header")
        .frame(
            egui::Frame::new()
                .fill(palette::SECONDARY)
                .inner_margin(egui::Margin {
                    left: 12,
                    right: 12,
                    top: 6,
                    bottom: 6,
                }),
        )
        .show(ctx, |ui| {
            // Bottom hairline under the header.
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("ADB Manager").strong().size(14.0));
                ui.label(
                    egui::RichText::new("ANDROID DEVICE TOOLKIT")
                        .small()
                        .color(palette::TEXT_FAINT),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_sized(
                            [30.0, 24.0],
                            egui::Button::new(egui::RichText::new("⚙").color(palette::TEXT_DIM)),
                        )
                        .on_hover_text("Settings")
                        .clicked()
                    {
                        state.page = Page::Settings;
                    }
                    device_selector(ui, state);
                });
            });
            ui.separator();
        });
}

fn device_selector(ui: &mut egui::Ui, state: &mut AppState) {
    let current = state.selected_device().cloned();
    let dot_color = current
        .as_ref()
        .map(|d| {
            if d.state.is_usable() {
                palette::SUCCESS
            } else {
                palette::WARNING
            }
        })
        .unwrap_or(palette::TEXT_FAINT);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.colored_label(dot_color, "●");
        let label = current
            .as_ref()
            .map(|d| d.display_name())
            .unwrap_or_else(|| "No device".to_string());
        egui::ComboBox::from_id_salt("device-selector")
            .selected_text(egui::RichText::new(label).strong())
            .width(250.0)
            .show_ui(ui, |ui| {
                if state.devices.is_empty() {
                    ui.label(egui::RichText::new("No devices found").color(palette::TEXT_DIM));
                }
                for d in state.devices.clone() {
                    // State travels in the row text so unauthorized/offline
                    // devices are identifiable before selection.
                    let row = format!(
                        "{}  ·  {}  ·  {}",
                        d.display_name(),
                        d.serial,
                        d.state.label()
                    );
                    ui.selectable_value(&mut state.selected_serial, Some(d.serial.clone()), row);
                }
                ui.separator();
                if ui
                    .add(egui::Button::new("+ Connect Device").fill(egui::Color32::TRANSPARENT))
                    .clicked()
                {
                    state.show_connect_dialog = true;
                    ui.close();
                }
            });
    });
}
