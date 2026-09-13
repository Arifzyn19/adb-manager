//! ADB Manager — production-quality Windows ADB toolkit in pure Rust.

mod adb;
mod app;
mod config;
mod device;
mod errors;
mod events;
mod logging;
mod state;
mod ui;

mod apk;
mod apps;
mod files;
mod logcat;
mod pairing;
mod processes;
mod shell;
mod tools;

use app::AdbManagerApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("ADB Manager"),
        ..Default::default()
    };
    eframe::run_native(
        "ADB Manager",
        options,
        Box::new(|cc| Ok(Box::new(AdbManagerApp::new(cc)) as Box<dyn eframe::App>)),
    )
}
