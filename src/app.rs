//! eframe application shell: owns state, workers, channels, menus.

use crate::adb::{detect_adb, AdbClient};
use crate::apps::{fetch_app_details, fetch_package_entries, AppActionKind};
use crate::config::AppConfig;
use crate::device::{fetch_info, DeviceManager, SavedDevices};
use crate::events::{AppEvent, Toast};
use crate::logcat::LogcatWorker;
use crate::shell::ShellWorker;
use crate::state::{AdbStatus, AppState, Page};
use crate::ui::{self, dialogs::ConnectDialogState};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

/// Process row action dispatched to a worker thread.
enum ProcessAction {
    ForceStop(String),
    Kill(u32),
}

/// Reconnect saved wireless devices in the background.
/// Loads fresh config + saved list so Settings toggles apply immediately.
fn spawn_auto_reconnect(adb_path: PathBuf, events: Sender<AppEvent>) {
    std::thread::spawn(move || {
        let config = AppConfig::load();
        if !(config.auto_reconnect && config.remember_devices) {
            return;
        }
        if crate::device::discovery::is_mock() {
            return;
        }
        let saved = SavedDevices::load();
        // Candidates are computed against an empty connected set here; the
        // poll loop reconciles a moment later and extra `adb connect` calls
        // against already-connected devices are harmless no-ops.
        for serial in saved.reconnect_candidates(&[]) {
            let client = AdbClient::new(adb_path.clone());
            match client.connect(&serial) {
                Ok(out) => {
                    let _ = events.send(AppEvent::Toast(Toast::success(format!(
                        "Reconnected {serial} ({out})"
                    ))));
                }
                Err(e) => {
                    tracing::info!("auto-reconnect {serial} failed: {e:#}");
                }
            }
        }
    });
}

pub struct AdbManagerApp {
    state: AppState,
    events_tx: Sender<AppEvent>,
    events_rx: Receiver<AppEvent>,
    device_manager: Option<DeviceManager>,
    connect_dlg: ConnectDialogState,
    logcat_worker: Option<LogcatWorker>,
    logcat_serial: Option<String>,
    shell_worker: Option<ShellWorker>,
    shell_serial: Option<String>,
    last_proc_fetch: Option<std::time::Instant>,
}

impl AdbManagerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::logging::init_logging();
        ui::theme::apply_theme(&cc.egui_ctx);

        let config = AppConfig::load();
        let saved = SavedDevices::load();
        let (tx, rx) = std::sync::mpsc::channel();

        let mut app = Self {
            state: AppState::new(config, saved),
            events_tx: tx,
            events_rx: rx,
            device_manager: None,
            connect_dlg: ConnectDialogState::default(),
            logcat_worker: None,
            logcat_serial: None,
            shell_worker: None,
            shell_serial: None,
            last_proc_fetch: None,
        };
        app.startup();
        app
    }

    /// Startup sequence: config → locate ADB → validate → start discovery.
    /// Never blocks the UI thread: detection runs on a worker thread.
    fn startup(&mut self) {
        // Mock mode shortcut.
        if std::env::var("ADB_MANAGER_MOCK")
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            self.state.adb_status = AdbStatus::Ready;
            self.state.adb_version = Some("1.0.41 (mock)".to_string());
            self.state.adb_message = "Mock mode".to_string();
            self.state.config.adb_path = Some("mock-adb".into());
            self.restart_device_worker();
            return;
        }

        // 1. Configured path wins if it still validates.
        if let Some(path) = self.state.config.adb_path.clone() {
            let tx = self.events_tx.clone();
            std::thread::spawn(move || match AdbClient::validate_path(&path) {
                Ok(version) => {
                    let _ = tx.send(AppEvent::AdbStatusChanged {
                        available: true,
                        version: Some(version),
                        message: "Ready".to_string(),
                    });
                    spawn_auto_reconnect(path, tx);
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::AdbStatusChanged {
                        available: false,
                        version: None,
                        message: e.guidance().to_string(),
                    });
                }
            });
            self.restart_device_worker();
            return;
        }

        // 2. Otherwise auto-detect in background.
        self.state.adb_message = "Detecting ADB…".to_string();
        let tx = self.events_tx.clone();
        std::thread::spawn(move || match detect_adb() {
            Ok((path, version)) => {
                let _ = tx.send(AppEvent::AdbStatusChanged {
                    available: true,
                    version: Some(version),
                    message: format!("Auto-detected {}", path.display()),
                });
                let _ = tx.send(AppEvent::Toast(Toast::success("ADB detected")));
                // Persist detection for next launch.
                let mut cfg = AppConfig::load();
                cfg.adb_path = Some(path.clone());
                let _ = cfg.save();
                spawn_auto_reconnect(path, tx);
            }
            Err(e) => {
                let _ = tx.send(AppEvent::AdbStatusChanged {
                    available: false,
                    version: None,
                    message: "ADB not found — configure it in Settings".to_string(),
                });
                let _ = tx.send(AppEvent::Error {
                    title: e.title().to_string(),
                    message: e.guidance().to_string(),
                    details: Some(e.to_string()),
                });
            }
        });
        // Worker starts with an empty path; it yields no devices until a
        // valid path arrives via the AdbStatusChanged → restart path below.
        // To keep Phase 1 simple, restart the worker once detection resolves
        // (handled in `on_adb_status_changed` via polling the saved config).
    }

    fn restart_device_worker(&mut self) {
        let Some(path) = self.state.config.adb_path.clone() else {
            return;
        };
        let interval = self.state.config.refresh_interval_secs;
        let tx = self.events_tx.clone();
        match self.device_manager.as_mut() {
            Some(m) => m.start(path, interval),
            None => {
                let mut m = DeviceManager::new(path.clone(), interval, tx);
                m.start(path, interval);
                self.device_manager = Some(m);
            }
        }
    }

    fn drain_events(&mut self) {
        // If ADB just became ready and we never started the worker (auto-detect
        // path), pick up the persisted path now.
        let mut need_restart = false;
        while let Ok(event) = self.events_rx.try_recv() {
            if let AppEvent::AdbStatusChanged { available, .. } = &event {
                if *available {
                    let fresh = AppConfig::load();
                    if fresh.adb_path.is_some() {
                        self.state.config.adb_path = fresh.adb_path;
                        if let Err(e) = self.state.config.save() {
                            tracing::warn!("saving auto-detected adb path: {e:#}");
                        }
                        need_restart = true;
                    }
                }
            }
            // Manual refresh event synthesizes nothing; handled via flag below.
            self.state.handle_event(event);
        }
        if need_restart && self.device_manager.is_none() {
            self.restart_device_worker();
        }
    }

    /// Fetch details for the selected device if missing (one worker per
    /// device; results arrive as `DeviceInfoUpdated`). Cheap to call per frame.
    fn maybe_fetch_selected_info(&mut self) {
        if self.state.adb_status != AdbStatus::Ready {
            return;
        }
        let Some(device) = self.state.selected_device().cloned() else {
            return;
        };
        if !device.state.is_usable() {
            return;
        }
        if self.state.info.contains_key(&device.serial)
            || self.state.info_pending.contains(&device.serial)
        {
            return;
        }
        let Some(adb_path) = self.state.config.adb_path.clone() else {
            return;
        };
        self.state.info_pending.insert(device.serial.clone());
        let tx = self.events_tx.clone();
        let serial = device.serial.clone();
        std::thread::spawn(move || {
            let info = fetch_info(&adb_path, &serial);
            if info.fetch_failed {
                let _ = tx.send(AppEvent::Error {
                    title: "Device details unavailable".to_string(),
                    message: format!(
                        "Could not read properties from {serial}. The device may have gone away."
                    ),
                    details: None,
                });
            }
            let _ = tx.send(AppEvent::DeviceInfoUpdated { serial, info });
        });
    }

    /// `adb disconnect SERIAL` on a worker thread (wireless devices).
    fn disconnect_device(&self, serial: String) {
        let Some(path) = self.state.config.adb_path.clone() else {
            return;
        };
        let tx = self.events_tx.clone();
        std::thread::spawn(move || match AdbClient::new(path).disconnect(&serial) {
            Ok(out) => {
                let _ = tx.send(AppEvent::Toast(Toast::success(format!(
                    "Disconnecting {serial} ({out})"
                ))));
            }
            Err(e) => {
                let _ = tx.send(AppEvent::Error {
                    title: e.title().to_string(),
                    message: e.guidance().to_string(),
                    details: Some(e.to_string()),
                });
            }
        });
    }

    /// `adb connect SERIAL` for a saved entry, then refresh its `last_seen`.
    fn connect_saved(&mut self, serial: String) {
        let Some(path) = self.state.config.adb_path.clone() else {
            self.state
                .push_toast(Toast::warning("No ADB path configured yet"));
            return;
        };
        let nickname = self
            .state
            .saved
            .devices
            .iter()
            .find(|d| d.serial == serial)
            .and_then(|d| d.nickname.clone());
        self.state.saved.remember_wireless(&serial, nickname);
        let tx = self.events_tx.clone();
        std::thread::spawn(move || match AdbClient::new(path).connect(&serial) {
            Ok(out) => {
                let _ = tx.send(AppEvent::Toast(Toast::success(format!(
                    "Connecting to {serial} ({out})"
                ))));
            }
            Err(e) => {
                let _ = tx.send(AppEvent::Error {
                    title: e.title().to_string(),
                    message: e.guidance().to_string(),
                    details: Some(e.to_string()),
                });
            }
        });
    }

    /// `adb pair ADDR CODE` on a worker thread; result arrives as
    /// `AppEvent::PairingResult` (state machine lives in `AppState`).
    fn pair_device(&self, req: crate::pairing::PairingRequest) {
        let Some(path) = self.state.config.adb_path.clone() else {
            return;
        };
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let addr = req.addr();
            match AdbClient::new(path).pair(&addr, &req.code) {
                Ok(out) => {
                    let _ = tx.send(AppEvent::PairingResult {
                        success: true,
                        message: out,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::PairingResult {
                        success: false,
                        message: e.to_string(),
                    });
                }
            }
        });
    }

    /// Fetch the app list for the selected device (fast `pm` pass, then
    /// per-package `dumpsys` resolution with progress). Skipped when a fetch
    /// is already in flight or data is cached.
    fn ensure_apps_fetch(&mut self) {
        if self.state.adb_status != AdbStatus::Ready {
            return;
        }
        let Some(device) = self.state.selected_device().cloned() else {
            return;
        };
        if !device.state.is_usable() {
            return;
        }
        let serial = device.serial.clone();
        let needs_list = match self.state.apps.get(&serial) {
            None => true,
            Some(cache) => cache.entries.is_empty() && !cache.list_loading,
        };
        if !needs_list {
            return;
        }
        let Some(path) = self.state.config.adb_path.clone() else {
            return;
        };
        if let Some(cache) = self.state.apps.get_mut(&serial) {
            cache.list_loading = true;
        } else {
            let mut cache = crate::state::AppsCache::default();
            cache.list_loading = true;
            self.state.apps.insert(serial.clone(), cache);
        }
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let client = AdbClient::new(path);
            match fetch_package_entries(&client, &serial) {
                Ok((entries, running)) => {
                    let total = entries.len();
                    let _ = tx.send(AppEvent::AppsPackages {
                        serial: serial.clone(),
                        entries: entries.clone(),
                        running: running.clone(),
                    });
                    // Slow pass: resolve labels/versions in package order.
                    let mut resolved = Vec::new();
                    for (i, entry) in entries.iter().enumerate() {
                        let running_now = running.contains(&entry.package);
                        if let Ok(info) = fetch_app_details(
                            &client,
                            &serial,
                            &entry.package,
                            entry.system,
                            running_now,
                        ) {
                            resolved.push(info);
                        }
                        if i % 10 == 0 || i + 1 == total {
                            let _ = tx.send(AppEvent::AppsProgress {
                                serial: serial.clone(),
                                done: i + 1,
                                total,
                            });
                        }
                    }
                    let _ = tx.send(AppEvent::AppsResolved {
                        serial,
                        apps: resolved,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::Error {
                        title: e.title().to_string(),
                        message: e.guidance().to_string(),
                        details: Some(e.to_string()),
                    });
                }
            }
        });
    }

    /// Re-fetch a single package's details (after actions / on demand).
    fn refresh_app_details(&self, serial: String, package: String, system: bool, running: bool) {
        let Some(path) = self.state.config.adb_path.clone() else {
            return;
        };
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let client = AdbClient::new(path);
            match fetch_app_details(&client, &serial, &package, system, running) {
                Ok(info) => {
                    let _ = tx.send(AppEvent::AppDetails { serial, info });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::Error {
                        title: e.title().to_string(),
                        message: e.guidance().to_string(),
                        details: Some(e.to_string()),
                    });
                }
            }
        });
    }

    /// Run a Launch/ForceStop/Clear/Uninstall action on a worker thread.
    fn run_app_action(&self, serial: String, package: String, kind: AppActionKind) {
        let Some(path) = self.state.config.adb_path.clone() else {
            return;
        };
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let client = AdbClient::new(path);
            let result = match kind {
                AppActionKind::Launch => crate::apps::launch_app(&client, &serial, &package),
                AppActionKind::ForceStop => crate::apps::force_stop_app(&client, &serial, &package),
                AppActionKind::ClearCache => {
                    crate::apps::clear_app(&client, &serial, &package, true)
                }
                AppActionKind::ClearData => {
                    crate::apps::clear_app(&client, &serial, &package, false)
                }
                AppActionKind::Uninstall => crate::apps::uninstall_app(&client, &serial, &package),
                AppActionKind::Extract => unreachable!("extract runs via extract_apk"),
            };
            match result {
                Ok(out) => {
                    let _ = tx.send(AppEvent::AppActionDone {
                        serial,
                        action: kind.label().to_string(),
                        package,
                        ok: true,
                        message: out,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::AppActionDone {
                        serial,
                        action: kind.label().to_string(),
                        package,
                        ok: false,
                        message: e.to_string(),
                    });
                }
            }
        });
    }

    /// Pull all APK splits for a package into the chosen Windows folder.
    fn extract_apk(&self, serial: String, package: String, dest: std::path::PathBuf) {
        let Some(path) = self.state.config.adb_path.clone() else {
            return;
        };
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let client = AdbClient::new(path);
            match crate::apps::pull_apk(&client, &serial, &package, &dest) {
                Ok(files) => {
                    let _ = tx.send(AppEvent::AppExtractDone {
                        serial,
                        package,
                        ok: true,
                        message: dest.display().to_string(),
                        files: files.iter().map(|p| p.display().to_string()).collect(),
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::AppExtractDone {
                        serial,
                        package,
                        ok: false,
                        message: e.to_string(),
                        files: Vec::new(),
                    });
                }
            }
        });
    }

    /// Keep exactly one logcat stream alive: the selected usable device.
    /// Runs regardless of the visible page so crash detection works in the
    /// background. Stops (and kills the child) on device/adb changes.
    fn ensure_logcat_stream(&mut self) {
        let wanted = if self.state.adb_status == AdbStatus::Ready {
            self.state
                .selected_device()
                .filter(|d| d.state.is_usable())
                .map(|d| d.serial.clone())
        } else {
            None
        };
        // Drop the stream when its device vanished from the list.
        if let Some(current) = self.logcat_serial.clone() {
            let gone = !self.state.devices.iter().any(|d| d.serial == current);
            if gone || wanted.as_deref() != Some(current.as_str()) {
                self.stop_logcat();
            }
        }
        if self.logcat_worker.is_some() {
            return;
        }
        let (Some(serial), Some(path)) = (wanted, self.state.config.adb_path.clone()) else {
            self.logcat_serial = None;
            return;
        };
        self.logcat_serial = Some(serial.clone());
        self.logcat_worker = Some(LogcatWorker::start(path, serial, self.events_tx.clone()));
    }

    fn stop_logcat(&mut self) {
        if let Some(mut worker) = self.logcat_worker.take() {
            worker.stop();
        }
        self.logcat_serial = None;
    }

    /// Start (or keep) exactly one shell session for the given device.
    /// A dead session is replaced; results arrive as Shell* events.
    fn ensure_shell(&mut self, serial: String) {
        if self.shell_serial.as_deref() == Some(serial.as_str()) && self.shell_worker.is_some() {
            return;
        }
        self.stop_shell();
        let Some(path) = self.state.config.adb_path.clone() else {
            self.state
                .push_toast(Toast::warning("No ADB path configured yet"));
            return;
        };
        // Fresh transcript ownership for the new device.
        self.state.shell.serial = Some(serial.clone());
        self.state.shell.blocks.clear();
        self.state.shell.running = None;
        self.state.shell.connected = false;
        self.state.shell.error = None;
        match ShellWorker::start(path, serial.clone(), self.events_tx.clone()) {
            Ok(worker) => {
                self.shell_worker = Some(worker);
                self.shell_serial = Some(serial);
            }
            Err(e) => {
                let _ = self.events_tx.send(AppEvent::ShellError {
                    serial,
                    message: e.to_string(),
                });
            }
        }
    }

    fn stop_shell(&mut self) {
        if let Some(mut worker) = self.shell_worker.take() {
            worker.stop();
        }
        self.shell_serial = None;
    }

    /// Send one command through the live session (respawning it first when
    /// it died or belongs to another device).
    fn send_shell(&mut self, cmd: String) {
        let Some(device) = self.state.selected_device().cloned() else {
            return;
        };
        if !device.state.is_usable() {
            return;
        }
        self.ensure_shell(device.serial.clone());
        let mut failed: Option<String> = None;
        if let Some(worker) = &self.shell_worker {
            if let Err(e) = worker.send(&cmd) {
                failed = Some(e.to_string());
            } else {
                self.state.shell.running = Some(cmd);
                return;
            }
        } else {
            failed = Some("Shell session could not start.".to_string());
        }
        // Send failed (died between ensure and write): drop the worker so
        // the next Send respawns, and surface the reason.
        self.stop_shell();
        self.state.shell.running = None;
        if let Some(message) = failed {
            self.state
                .push_toast(Toast::error(format!("Shell send failed: {message}")));
        }
    }

    /// Inspect one APK file on a worker thread (ZIP + AXML decode).
    /// Result arrives as `ApkInspected` / `ApkInspectFailed`.
    fn inspect_apk(&mut self, path: std::path::PathBuf) {
        let display = path.display().to_string();
        self.state.apk.path = Some(display.clone());
        self.state.apk.info = None;
        self.state.apk.error = None;
        self.state.apk.loading = true;
        let tx = self.events_tx.clone();
        std::thread::spawn(move || match crate::apk::inspect_apk(&path) {
            Ok(info) => {
                let _ = tx.send(AppEvent::ApkInspected {
                    path: display,
                    info: Box::new(info),
                });
            }
            Err(e) => {
                let _ = tx.send(AppEvent::ApkInspectFailed {
                    path: display,
                    message: e.to_string(),
                });
            }
        });
    }

    /// `adb install [-r]` / `install-multiple [-r]` on a worker thread.
    fn install_apk(&mut self, req: crate::ui::apk::InstallRequest) {
        let Some(path) = self.state.config.adb_path.clone() else {
            self.state
                .push_toast(Toast::warning("No ADB path configured yet"));
            return;
        };
        self.state.apk.installing = true;
        self.state.apk.last_install = None;
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let client = AdbClient::new(path);
            match crate::apk::install_apks(
                &client,
                &req.serial,
                &req.files,
                req.reinstall,
                &req.display,
            ) {
                Ok(out) => {
                    let _ = tx.send(AppEvent::ApkInstallDone {
                        serial: req.serial,
                        files: req.files,
                        ok: true,
                        message: out,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::ApkInstallDone {
                        serial: req.serial,
                        files: req.files,
                        ok: false,
                        message: e.to_string(),
                    });
                }
            }
        });
    }
    /// List a remote directory on a worker thread.
    /// Results arrive as `FilesListed` / `FilesError` (stale ones dropped).
    fn fetch_files(&mut self, serial: String, dir: String) {
        // Silent until ADB is ready (the global status bar + onboarding own
        // that messaging — no per-frame toast spam from the poll loop).
        if self.state.adb_status != AdbStatus::Ready {
            return;
        }
        let Some(adb_path) = self.state.config.adb_path.clone() else {
            return;
        };
        self.state.files.serial = Some(serial.clone());
        self.state.files.pending = Some(dir.clone());
        self.state.files.loading = true;
        self.state.files.error = None;
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let client = AdbClient::new(adb_path);
            match crate::files::list_dir(&client, &serial, &dir) {
                Ok((canonical, entries)) => {
                    let _ = tx.send(AppEvent::FilesListed {
                        serial,
                        dir: canonical,
                        entries,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::FilesError {
                        serial,
                        dir,
                        message: e.to_string(),
                    });
                }
            }
        });
    }

    /// Run a file mutation / transfer on a worker thread; the listing
    /// refreshes automatically on success (`refresh_list` marker).
    fn run_file_op(&mut self, serial: String, op: crate::ui::files::FileOp) {
        let Some(adb_path) = self.state.config.adb_path.clone() else {
            self.state
                .push_toast(Toast::warning("No ADB path configured yet"));
            return;
        };
        let cwd = if self.state.files.cwd.is_empty() {
            self.state.config.files_root.clone()
        } else {
            self.state.files.cwd.clone()
        };
        let busy = match &op {
            crate::ui::files::FileOp::Mkdir(name) => format!("Create folder {name}"),
            crate::ui::files::FileOp::Delete(path) => format!("Delete {path}"),
            crate::ui::files::FileOp::Rename { from, .. } => format!("Rename {from}"),
            crate::ui::files::FileOp::Upload(locals) => {
                format!("Upload {} file(s)", locals.len())
            }
            crate::ui::files::FileOp::Download { remote, .. } => format!("Download {remote}"),
        };
        self.state.files.busy = Some(busy);
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let client = AdbClient::new(adb_path);
            let (op_label, target, result) = match op {
                crate::ui::files::FileOp::Mkdir(name) => (
                    "Create folder",
                    format!("{cwd}/{name}"),
                    crate::files::make_dir(&client, &serial, &cwd, &name),
                ),
                crate::ui::files::FileOp::Delete(path) => (
                    "Delete",
                    path.clone(),
                    crate::files::delete_path(&client, &serial, &path),
                ),
                crate::ui::files::FileOp::Rename { from, to } => (
                    "Rename",
                    format!("{from} → {to}"),
                    crate::files::rename_path(&client, &serial, &from, &to),
                ),
                crate::ui::files::FileOp::Upload(locals) => {
                    let n = locals.len();
                    (
                        "Upload",
                        format!("{n} file(s) → {cwd}"),
                        crate::files::upload_files(&client, &serial, &locals, &cwd)
                            .map(|v| v.join(", ")),
                    )
                }
                crate::ui::files::FileOp::Download { remote, local_dir } => (
                    "Download",
                    format!("{remote} → {local_dir}"),
                    crate::files::download_entry(&client, &serial, &remote, &local_dir),
                ),
            };
            match result {
                Ok(out) => {
                    let _ = tx.send(AppEvent::FileOpDone {
                        serial,
                        op: op_label.to_string(),
                        target,
                        ok: true,
                        message: out,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::FileOpDone {
                        serial,
                        op: op_label.to_string(),
                        target,
                        ok: false,
                        message: e.to_string(),
                    });
                }
            }
        });
    }

    /// Fetch battery + memory + storage + properties in one worker thread,
    /// emitting one event per snapshot plus a final `ToolInfoDone`.
    fn fetch_tools_info(&mut self, serial: String) {
        if self.state.adb_status != AdbStatus::Ready {
            return;
        }
        let Some(path) = self.state.config.adb_path.clone() else {
            return;
        };
        self.state.tools.serial = Some(serial.clone());
        self.state.tools.info_loading = true;
        self.state.tools.info_error = None;
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let client = AdbClient::new(path);
            let mut failed = false;
            match crate::tools::fetch_battery(&client, &serial) {
                Ok(info) => {
                    let _ = tx.send(AppEvent::ToolBattery {
                        serial: serial.clone(),
                        info,
                    });
                }
                Err(e) => {
                    failed = true;
                    let _ = tx.send(AppEvent::ToolInfoError {
                        serial: serial.clone(),
                        kind: "Battery".to_string(),
                        message: e.to_string(),
                    });
                }
            }
            match crate::tools::fetch_memory(&client, &serial) {
                Ok(info) => {
                    let _ = tx.send(AppEvent::ToolMemory {
                        serial: serial.clone(),
                        info,
                    });
                }
                Err(e) => {
                    failed = true;
                    let _ = tx.send(AppEvent::ToolInfoError {
                        serial: serial.clone(),
                        kind: "Memory".to_string(),
                        message: e.to_string(),
                    });
                }
            }
            match crate::tools::fetch_storage(&client, &serial) {
                Ok(entries) => {
                    let _ = tx.send(AppEvent::ToolStorage {
                        serial: serial.clone(),
                        entries,
                    });
                }
                Err(e) => {
                    failed = true;
                    let _ = tx.send(AppEvent::ToolInfoError {
                        serial: serial.clone(),
                        kind: "Storage".to_string(),
                        message: e.to_string(),
                    });
                }
            }
            match crate::tools::fetch_properties(&client, &serial) {
                Ok(props) => {
                    let _ = tx.send(AppEvent::ToolProps {
                        serial: serial.clone(),
                        props,
                    });
                }
                Err(e) => {
                    failed = true;
                    let _ = tx.send(AppEvent::ToolInfoError {
                        serial: serial.clone(),
                        kind: "Properties".to_string(),
                        message: e.to_string(),
                    });
                }
            }
            if !failed {
                let _ = tx.send(AppEvent::ToolInfoDone { serial });
            }
        });
    }

    /// Screenshot / recording / reboot / ADB-maintenance worker dispatch.
    fn run_tool_op(&mut self, op: crate::ui::tools::ToolOp) {
        use crate::ui::tools::ToolOp;
        let selected = self.state.selected_device().cloned();
        let Some(path) = self.state.config.adb_path.clone() else {
            self.state
                .push_toast(Toast::warning("No ADB path configured yet"));
            return;
        };
        match op {
            ToolOp::RefreshInfo => {
                if let Some(device) = selected {
                    if device.state.is_usable() {
                        // Force re-fetch even with cached snapshots.
                        self.state.tools.battery = None;
                        self.state.tools.info_error = None;
                        self.fetch_tools_info(device.serial);
                    }
                }
            }
            ToolOp::Screenshot => {
                let Some(device) = selected else { return };
                self.state.tools.shot_busy = true;
                let tx = self.events_tx.clone();
                std::thread::spawn(move || {
                    let client = AdbClient::new(path);
                    match crate::tools::take_screenshot(&client, &device.serial) {
                        Ok(png) => {
                            let _ = tx.send(AppEvent::ScreenshotDone {
                                serial: device.serial,
                                ok: true,
                                message: format!("{} bytes", png.len()),
                                local: None,
                                png: Some(png),
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::ScreenshotDone {
                                serial: device.serial,
                                ok: false,
                                message: e.to_string(),
                                local: None,
                                png: None,
                            });
                        }
                    }
                });
            }
            ToolOp::SaveShot(dest) => {
                if let Some(png) = self.state.tools.shot_png.clone() {
                    match std::fs::write(&dest, &png) {
                        Ok(()) => {
                            self.state.tools.shot_local = Some(dest.clone());
                            self.state
                                .push_toast(Toast::success(format!("Screenshot saved to {dest}")));
                        }
                        Err(e) => {
                            self.state
                                .push_toast(Toast::error(format!("Saving screenshot failed: {e}")));
                        }
                    }
                }
            }
            ToolOp::StartRec { secs, local } => {
                let Some(device) = selected else { return };
                if !device.state.is_usable() {
                    return;
                }
                // Remote scratch path from timestamp + serial stem.
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs().to_string())
                    .unwrap_or_else(|_| "rec".to_string());
                let stem: String = device
                    .serial
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                let remote = crate::tools::recording_remote_path(&stem, &ts);
                self.state.tools.rec_phase = crate::state::RecPhase::Recording;
                self.state.tools.rec_remote = Some(remote.clone());
                self.state.tools.rec_local = Some(local.clone());
                self.state.tools.rec_started = Some(std::time::Instant::now());
                self.state.tools.rec_error = None;
                let tx = self.events_tx.clone();
                std::thread::spawn(move || {
                    let client = AdbClient::new(path);
                    let serial = device.serial.clone();
                    let run = crate::tools::run_recording(&client, &serial, secs, &remote);
                    if let Err(e) = run {
                        // Interrupted early counts as failure here; the Stop
                        // path pulls whatever finalized before reporting.
                        let _ = tx.send(AppEvent::RecordingDone {
                            serial: serial.clone(),
                            ok: false,
                            message: e.to_string(),
                            local: None,
                        });
                        return;
                    }
                    match crate::tools::pull_recording(&client, &serial, &remote, &local) {
                        Ok(out) => {
                            crate::tools::cleanup_recording(&client, &serial, &remote);
                            let _ = tx.send(AppEvent::RecordingDone {
                                serial,
                                ok: true,
                                message: out,
                                local: Some(local),
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::RecordingDone {
                                serial,
                                ok: false,
                                message: e.to_string(),
                                local: None,
                            });
                        }
                    }
                });
            }
            ToolOp::StopRec => {
                let Some(device) = selected else { return };
                let tx = self.events_tx.clone();
                std::thread::spawn(move || {
                    let client = AdbClient::new(path);
                    match crate::tools::stop_recording(&client, &device.serial) {
                        Ok(out) => {
                            let _ = tx.send(AppEvent::Toast(Toast::info(out)));
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::Error {
                                title: e.title().to_string(),
                                message: e.guidance().to_string(),
                                details: Some(e.to_string()),
                            });
                        }
                    }
                });
            }
            ToolOp::Reboot(mode) => {
                let Some(device) = selected else { return };
                self.state.tools.busy = Some(mode.label().to_string());
                let tx = self.events_tx.clone();
                std::thread::spawn(move || {
                    let client = AdbClient::new(path);
                    match crate::tools::reboot_device(&client, &device.serial, mode) {
                        Ok(out) => {
                            let _ = tx.send(AppEvent::ToolActionDone {
                                serial: device.serial,
                                action: mode.label().to_string(),
                                ok: true,
                                message: out,
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::ToolActionDone {
                                serial: device.serial,
                                action: mode.label().to_string(),
                                ok: false,
                                message: e.to_string(),
                            });
                        }
                    }
                });
            }
            ToolOp::RestartAdb => {
                self.state.tools.busy = Some("Restart ADB".to_string());
                let tx = self.events_tx.clone();
                std::thread::spawn(move || {
                    // No serial: host-side server restart.
                    let adb_path = AppConfig::load().adb_path;
                    let client = adb_path
                        .map(AdbClient::new)
                        .unwrap_or_else(|| AdbClient::new(PathBuf::from("adb")));
                    match crate::tools::restart_adb(&client) {
                        Ok(out) => {
                            let _ = tx.send(AppEvent::ToolActionDone {
                                serial: String::new(),
                                action: "Restart ADB".to_string(),
                                ok: true,
                                message: out,
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::ToolActionDone {
                                serial: String::new(),
                                action: "Restart ADB".to_string(),
                                ok: false,
                                message: e.to_string(),
                            });
                        }
                    }
                });
            }
            ToolOp::ClearLogcat => {
                let Some(device) = selected else { return };
                self.state.tools.busy = Some("Clear Logcat".to_string());
                let tx = self.events_tx.clone();
                std::thread::spawn(move || {
                    let client = AdbClient::new(path);
                    match crate::tools::clear_logcat(&client, &device.serial) {
                        Ok(out) => {
                            let _ = tx.send(AppEvent::ToolActionDone {
                                serial: device.serial,
                                action: "Clear Logcat".to_string(),
                                ok: true,
                                message: out,
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::ToolActionDone {
                                serial: device.serial,
                                action: "Clear Logcat".to_string(),
                                ok: false,
                                message: e.to_string(),
                            });
                        }
                    }
                });
            }
        }
    }
    /// Fetch the process list once (unless already loading). Called on page
    /// enter, manual refresh, and every few seconds when auto-refresh is on.
    fn fetch_processes_once(&mut self) {
        if self.state.adb_status != AdbStatus::Ready {
            return;
        }
        let Some(device) = self.state.selected_device().cloned() else {
            return;
        };
        if !device.state.is_usable() {
            return;
        }
        let serial = device.serial.clone();
        if self.state.processes.get(&serial).is_some_and(|c| c.loading) {
            return;
        }
        let Some(path) = self.state.config.adb_path.clone() else {
            return;
        };
        self.state
            .processes
            .entry(serial.clone())
            .or_default()
            .loading = true;
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let client = AdbClient::new(path);
            match crate::processes::fetch_processes(&client, &serial) {
                Ok(procs) => {
                    let _ = tx.send(AppEvent::ProcessesUpdated { serial, procs });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::Error {
                        title: e.title().to_string(),
                        message: e.guidance().to_string(),
                        details: Some(e.to_string()),
                    });
                    // Unstick the loading flag via an empty update.
                    let _ = tx.send(AppEvent::ProcessesUpdated {
                        serial,
                        procs: Vec::new(),
                    });
                }
            }
        });
        self.last_proc_fetch = Some(std::time::Instant::now());
    }

    /// Force-stop (package) or SIGKILL (pid) on a worker thread.
    fn run_process_action(&self, serial: String, kind: ProcessAction) {
        let Some(path) = self.state.config.adb_path.clone() else {
            return;
        };
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let client = AdbClient::new(path);
            let (action, target, result) = match kind {
                ProcessAction::ForceStop(package) => (
                    "Force Stop",
                    package.clone(),
                    crate::apps::force_stop_app(&client, &serial, &package),
                ),
                ProcessAction::Kill(pid) => (
                    "Kill",
                    format!("PID {pid}"),
                    crate::processes::kill_process(&client, &serial, pid),
                ),
            };
            match result {
                Ok(out) => {
                    let _ = tx.send(AppEvent::ProcessActionDone {
                        serial,
                        action: action.to_string(),
                        target,
                        ok: true,
                        message: out,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::ProcessActionDone {
                        serial,
                        action: action.to_string(),
                        target,
                        ok: false,
                        message: e.to_string(),
                    });
                }
            }
        });
    }

    fn show_toasts(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toasts").show(ctx, |ui| {
            for t in &self.state.toasts {
                let (icon, color) = match t.toast.kind {
                    crate::events::ToastKind::Success => ("✓", egui::Color32::GREEN),
                    crate::events::ToastKind::Info => ("ℹ", egui::Color32::LIGHT_BLUE),
                    crate::events::ToastKind::Warning => ("⚠", egui::Color32::YELLOW),
                    crate::events::ToastKind::Error => ("✕", egui::Color32::RED),
                };
                ui.horizontal(|ui| {
                    ui.colored_label(color, icon);
                    ui.label(&t.toast.message);
                });
            }
        });
    }
}

impl eframe::App for AdbManagerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.state.tick_toasts(ctx.input(|i| i.stable_dt).min(0.1));
        self.maybe_fetch_selected_info();
        self.ensure_logcat_stream();

        // Background polling drives repaints.
        if self.state.config.auto_refresh {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        // First-run ADB onboarding (dismissible, explorable without ADB).
        if self.state.adb_status == AdbStatus::Unavailable && !self.state.first_run_dismissed {
            egui::Window::new("Welcome to ADB Manager")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("ADB / Android Platform Tools were not found.");
                    ui.label("Select your adb executable to continue.");
                    ui.horizontal(|ui| {
                        if ui.button("Open Settings").clicked() {
                            self.state.page = Page::Settings;
                            self.state.first_run_dismissed = true;
                        }
                        if ui.button("Continue without device").clicked() {
                            self.state.first_run_dismissed = true;
                        }
                    });
                });
        }

        ui::header::show(ctx, &mut self.state);
        ui::status_bar::show(ctx, &self.state);
        if !self.state.toasts.is_empty() {
            self.show_toasts(ctx);
        }

        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(190.0)
            .show(ctx, |ui| {
                ui::sidebar::show(ui, &mut self.state.page);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| match self.state.page {
                Page::Dashboard => ui::dashboard::show(ui, &mut self.state),
                Page::Devices => {
                    let a = ui::devices::show(ui, &mut self.state);
                    if a.refresh_requested {
                        self.restart_device_worker();
                    }
                    if let Some(serial) = a.disconnect_serial {
                        self.disconnect_device(serial);
                    }
                    if let Some(serial) = a.connect_serial {
                        self.connect_saved(serial);
                    }
                    if let Some(serial) = a.refresh_info_serial {
                        self.state.info.remove(&serial);
                        self.state.info_pending.remove(&serial);
                    }
                    if a.saved_changed {
                        self.state.saved.save();
                    }
                }
                Page::Apps => {
                    self.ensure_apps_fetch();
                    // Consume refresh markers left by AppActionDone.
                    if self.state.apps_view.refresh_list {
                        self.state.apps_view.refresh_list = false;
                        if let Some(serial) = self.state.selected_serial.clone() {
                            self.state.apps.remove(&serial);
                        }
                        self.ensure_apps_fetch();
                    }
                    if let Some(pkg) = self.state.apps_view.refresh_details.take() {
                        if let Some(serial) = self.state.selected_serial.clone() {
                            let (system, running) = self
                                .state
                                .apps
                                .get(&serial)
                                .map(|c| {
                                    (
                                        c.entries
                                            .iter()
                                            .find(|e| e.package == pkg)
                                            .is_some_and(|e| e.system),
                                        c.running.contains(&pkg),
                                    )
                                })
                                .unwrap_or((false, false));
                            self.refresh_app_details(serial, pkg, system, running);
                        }
                    }
                    let a = ui::apps::show(ui, &mut self.state);
                    if a.refresh_requested {
                        if let Some(serial) = self.state.selected_serial.clone() {
                            self.state.apps.remove(&serial);
                        }
                        self.ensure_apps_fetch();
                    }
                    if let Some((package, system, running)) = a.details_requested {
                        if let Some(serial) = self.state.selected_serial.clone() {
                            self.refresh_app_details(serial, package, system, running);
                        }
                    }
                    if let Some((package, kind)) = a.action_requested {
                        if let Some(serial) = self.state.selected_serial.clone() {
                            let tag = format!("{package}:{}", kind.label());
                            self.state.apps_view.busy = Some(tag);
                            if kind == crate::apps::AppActionKind::Extract {
                                if let Some(dir) = rfd::FileDialog::new()
                                    .set_title("Choose extraction folder")
                                    .pick_folder()
                                {
                                    self.extract_apk(serial, package, dir);
                                } else {
                                    self.state.apps_view.busy = None;
                                }
                            } else {
                                self.run_app_action(serial, package, kind);
                            }
                        }
                    }
                }
                Page::Processes => {
                    // Initial fetch when the cache is empty.
                    let serial = self.state.selected_serial.clone();
                    let needs_initial = serial
                        .as_deref()
                        .is_some_and(|s| !self.state.processes.contains_key(s));
                    if needs_initial {
                        self.fetch_processes_once();
                    }
                    // Consume post-action refresh marker.
                    if self.state.proc_view.refresh_list {
                        self.state.proc_view.refresh_list = false;
                        self.fetch_processes_once();
                    }
                    // Auto-refresh every 5 s while watching the page.
                    if self.state.proc_view.auto_refresh {
                        let due = self
                            .last_proc_fetch
                            .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(5));
                        if due {
                            self.fetch_processes_once();
                        }
                    }
                    let a = ui::processes::show(ctx, ui, &mut self.state);
                    if a.refresh_requested {
                        self.fetch_processes_once();
                    }
                    if let Some(action) = a.action_requested {
                        if let Some(serial) = self.state.selected_serial.clone() {
                            match action {
                                ui::processes::ProcAction::ForceStop(package) => {
                                    self.state.proc_view.busy =
                                        Some(format!("Force Stop {package}"));
                                    self.run_process_action(
                                        serial,
                                        ProcessAction::ForceStop(package),
                                    );
                                }
                                ui::processes::ProcAction::Kill(pid) => {
                                    self.state.proc_view.busy = Some(format!("Kill PID {pid}"));
                                    self.run_process_action(serial, ProcessAction::Kill(pid));
                                }
                            }
                        }
                    }
                }
                Page::Files => {
                    // Keep the listing pinned to the selected device.
                    if let Some(device) = self.state.selected_device().cloned() {
                        let root = self.state.config.files_root.clone();
                        let switched =
                            self.state.files.serial.as_deref() != Some(device.serial.as_str());
                        if switched {
                            self.state.files.serial = Some(device.serial.clone());
                            self.state.files.cwd.clear();
                            self.state.files.pending = None;
                            self.state.files.entries.clear();
                            self.state.files.error = None;
                            self.state.files.busy = None;
                        }
                        // Initial + post-op + manual refresh fetches.
                        let mut want: Option<String> = None;
                        if self.state.files.pending.is_none()
                            && self.state.files.entries.is_empty()
                            && !self.state.files.loading
                            && self.state.files.error.is_none()
                        {
                            want = Some(if self.state.files.cwd.is_empty() {
                                root
                            } else {
                                self.state.files.cwd.clone()
                            });
                        }
                        if self.state.files.refresh_list {
                            self.state.files.refresh_list = false;
                            want = Some(if self.state.files.cwd.is_empty() {
                                root
                            } else {
                                self.state.files.cwd.clone()
                            });
                        }
                        if device.state.is_usable() {
                            if let Some(dir) = want {
                                self.fetch_files(device.serial.clone(), dir);
                            }
                        }
                    }
                    let a = ui::files::show(ctx, ui, &mut self.state);
                    if a.refresh {
                        if let Some(serial) = self.state.selected_serial.clone() {
                            let dir = if self.state.files.cwd.is_empty() {
                                self.state.config.files_root.clone()
                            } else {
                                self.state.files.cwd.clone()
                            };
                            self.fetch_files(serial, dir);
                        }
                    }
                    if let Some(dir) = a.navigate {
                        if let Some(serial) = self.state.selected_serial.clone() {
                            self.fetch_files(serial, dir);
                        }
                    }
                    if let Some(op) = a.op {
                        if let Some(serial) = self.state.selected_serial.clone() {
                            self.run_file_op(serial, op);
                        }
                    }
                }
                Page::Logcat => ui::logcat::show(ctx, ui, &mut self.state),
                Page::Apk => {
                    let a = ui::apk::show(ctx, ui, &mut self.state);
                    if let Some(path) = a.inspect_path {
                        self.inspect_apk(path);
                    }
                    if let Some(req) = a.install {
                        self.install_apk(req);
                    }
                }
                Page::Shell => {
                    // One live session per selected usable device.
                    if let Some(device) = self.state.selected_device().cloned() {
                        if device.state.is_usable() {
                            self.ensure_shell(device.serial.clone());
                        } else {
                            self.stop_shell();
                        }
                    } else {
                        self.stop_shell();
                    }
                    let a = ui::shell::show(ctx, ui, &mut self.state);
                    if a.stop {
                        self.stop_shell();
                        self.state.shell.running = None;
                        self.state.shell.connected = false;
                    }
                    if let Some(cmd) = a.send {
                        self.send_shell(cmd);
                    }
                }
                Page::Tools => {
                    // Pin snapshots to the selected device; fetch once.
                    if let Some(device) = self.state.selected_device().cloned() {
                        if self.state.tools.serial.as_deref() != Some(device.serial.as_str()) {
                            self.state.tools.serial = Some(device.serial.clone());
                            self.state.tools.battery = None;
                            self.state.tools.memory = None;
                            self.state.tools.storage.clear();
                            self.state.tools.props.clear();
                            self.state.tools.info_error = None;
                            self.state.tools.shot_png = None;
                            self.state.tools.shot_local = None;
                            self.state.tools.rec_phase = crate::state::RecPhase::Idle;
                            self.state.tools.rec_remote = None;
                            self.state.tools.rec_local = None;
                        }
                        if device.state.is_usable()
                            && !self.state.tools.info_loading
                            && self.state.tools.battery.is_none()
                            && self.state.tools.info_error.is_none()
                        {
                            self.fetch_tools_info(device.serial.clone());
                        }
                    }
                    let a = ui::tools::show(ctx, ui, &mut self.state);
                    if let Some(op) = a.op {
                        self.run_tool_op(op);
                    }
                }
                Page::Settings => {
                    let a = ui::settings::show(ui, &mut self.state, &self.events_tx);
                    if a.config_changed {
                        if let Err(e) = self.state.config.save() {
                            tracing::warn!("saving config: {e:#}");
                        }
                    }
                    if let Some(path) = a.adb_path_changed {
                        // Re-validate + restart discovery against the new path.
                        match AdbClient::validate_path(&path) {
                            Ok(version) => {
                                self.state.adb_status = AdbStatus::Ready;
                                self.state.adb_version = Some(version);
                                self.state.adb_message = "Ready".to_string();
                                spawn_auto_reconnect(path.clone(), self.events_tx.clone());
                            }
                            Err(e) => {
                                self.state.set_adb_error(&e);
                            }
                        }
                        self.restart_device_worker();
                        // The logcat child belongs to the old adb: restart it.
                        self.stop_logcat();
                        self.stop_shell();
                    }
                }
            });
        });

        if self.state.show_connect_dialog {
            let a = ui::dialogs::show(ctx, &mut self.state, &mut self.connect_dlg, &self.events_tx);
            if let Some(req) = a.pair_request {
                self.pair_device(req);
            }
        }

        // Palette window (keyboard-first); actions applied below.
        if self.state.palette_open {
            if let Some(action) = ui::palette::show(ctx, &mut self.state) {
                match action {
                    ui::palette::PaletteAction::Goto(page) => {
                        self.state.page = page;
                    }
                    ui::palette::PaletteAction::Connect => {
                        self.state.show_connect_dialog = true;
                    }
                    ui::palette::PaletteAction::Refresh => {
                        self.restart_device_worker();
                    }
                }
            }
        }

        // Ctrl+R → restart discovery; Ctrl+K → palette; Ctrl+Shift+L/A/S →
        // Logcat/Apps/Shell. (No plain Ctrl+A/C/V/X: those stay with text
        // editing, per §46.)
        if ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::R,
            ))
        }) {
            self.restart_device_worker();
        }
        if ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::K,
            ))
        }) {
            self.state.palette_open = !self.state.palette_open;
            self.state.palette_query.clear();
            self.state.palette_idx = 0;
        }
        let shift_ctrl = egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT);
        for (key, page) in [
            (egui::Key::L, Page::Logcat),
            (egui::Key::A, Page::Apps),
            (egui::Key::S, Page::Shell),
        ] {
            if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(shift_ctrl, key)))
            {
                self.state.page = page;
            }
        }
    }
}
