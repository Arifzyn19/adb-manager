//! Command palette (Phase 11): Ctrl+K fuzzy-ish action search.
//!
//! Pages, Connect Device and Refresh live here so power users never touch
//! the mouse. The window is keyboard-first: type to filter, Enter runs the
//! top hit, Esc closes.

use crate::state::{AppState, Page};

/// Intent returned to `app.rs` (navigation + discovery live there).
pub enum PaletteAction {
    Goto(Page),
    Connect,
    Refresh,
}

struct Item {
    title: &'static str,
    hint: &'static str,
    action: PaletteActionKind,
}

#[derive(Clone, Copy)]
enum PaletteActionKind {
    Goto(Page),
    Connect,
    Refresh,
}

const ITEMS: &[Item] = &[
    Item {
        title: "Dashboard",
        hint: "overview",
        action: PaletteActionKind::Goto(Page::Dashboard),
    },
    Item {
        title: "Devices",
        hint: "usb wireless",
        action: PaletteActionKind::Goto(Page::Devices),
    },
    Item {
        title: "Apps",
        hint: "packages install",
        action: PaletteActionKind::Goto(Page::Apps),
    },
    Item {
        title: "Processes",
        hint: "ps kill cpu",
        action: PaletteActionKind::Goto(Page::Processes),
    },
    Item {
        title: "Files",
        hint: "sdcard push pull",
        action: PaletteActionKind::Goto(Page::Files),
    },
    Item {
        title: "Logcat",
        hint: "logs crash",
        action: PaletteActionKind::Goto(Page::Logcat),
    },
    Item {
        title: "APK",
        hint: "inspector installer",
        action: PaletteActionKind::Goto(Page::Apk),
    },
    Item {
        title: "Shell",
        hint: "terminal",
        action: PaletteActionKind::Goto(Page::Shell),
    },
    Item {
        title: "Device Tools",
        hint: "screenshot battery reboot",
        action: PaletteActionKind::Goto(Page::Tools),
    },
    Item {
        title: "Settings",
        hint: "adb config",
        action: PaletteActionKind::Goto(Page::Settings),
    },
    Item {
        title: "Connect Device…",
        hint: "pair wireless usb",
        action: PaletteActionKind::Connect,
    },
    Item {
        title: "Refresh Devices",
        hint: "rediscover",
        action: PaletteActionKind::Refresh,
    },
];

/// Filter items by a space-separated AND query over title + hint.
pub fn filter_items(query: &str) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    ITEMS
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            let hay = format!("{} {}", item.title, item.hint).to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .map(|(i, _)| i)
        .collect()
}

pub fn show(ctx: &egui::Context, state: &mut AppState) -> Option<PaletteAction> {
    let mut action: Option<PaletteAction> = None;

    // Esc closes from anywhere while open.
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.palette_open = false;
        return None;
    }

    egui::Window::new("⌕ Command palette")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_min_width(380.0);
            ui.add(
                egui::TextEdit::singleline(&mut state.palette_query)
                    .hint_text("Type a command — pages, connect, refresh…"),
            )
            .request_focus();

            let hits = filter_items(&state.palette_query);
            if hits.is_empty() {
                ui.label(
                    egui::RichText::new("No matches.").color(crate::ui::theme::palette::TEXT_DIM),
                );
                return;
            }
            // Clamp the cursor into the hit list.
            if state.palette_idx >= hits.len() {
                state.palette_idx = 0;
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                state.palette_idx = (state.palette_idx + 1) % hits.len();
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                state.palette_idx = state.palette_idx.saturating_sub(1);
            }
            let mut run: Option<usize> = None;
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                run = Some(state.palette_idx);
            }
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    for (row, idx) in hits.iter().enumerate() {
                        let item = &ITEMS[*idx];
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(row == state.palette_idx, item.title)
                                .clicked()
                            {
                                run = Some(row);
                            }
                            ui.label(
                                egui::RichText::new(item.hint)
                                    .small()
                                    .color(crate::ui::theme::palette::TEXT_FAINT),
                            );
                        });
                    }
                });
            if let Some(row) = run {
                if let Some(idx) = hits.get(row) {
                    match ITEMS[*idx].action {
                        PaletteActionKind::Goto(page) => {
                            action = Some(PaletteAction::Goto(page));
                        }
                        PaletteActionKind::Connect => {
                            action = Some(PaletteAction::Connect);
                        }
                        PaletteActionKind::Refresh => {
                            action = Some(PaletteAction::Refresh);
                        }
                    }
                    state.palette_open = false;
                    state.palette_query.clear();
                    state.palette_idx = 0;
                }
            }
        });

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_lists_everything() {
        assert_eq!(filter_items("").len(), ITEMS.len());
    }

    #[test]
    fn matches_title_and_hint_words() {
        let hits = filter_items("log");
        assert!(hits.iter().any(|i| ITEMS[*i].title == "Logcat"));
        let hits = filter_items("screenshot");
        assert!(hits.iter().any(|i| ITEMS[*i].title == "Device Tools"));
    }

    #[test]
    fn multi_word_and_semantics() {
        assert!(filter_items("device tools")
            .iter()
            .any(|i| ITEMS[*i].title == "Device Tools"));
        assert!(filter_items("device zebra").is_empty());
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(filter_items("SHELL").len(), filter_items("shell").len());
    }
}
