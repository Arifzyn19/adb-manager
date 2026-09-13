//! Central application state (UI thread only).
//!
//! Workers communicate with this via [`AppEvent`](crate::events::AppEvent);
//! state mutation happens in exactly one place: `AppState::handle_event`.

use crate::adb::{AdbError, Device};
use crate::apk::ApkInfo;
use crate::apps::{AppFilter, AppInfo, PackageEntry};
use crate::config::AppConfig;
use crate::device::{DeviceInfo, SavedDevices};
use crate::events::{AppEvent, Toast};
use crate::files::FileEntry;
use crate::logcat::{CrashReport, LogBuffer, LogViewFilter};
use crate::pairing::PairingState;
use crate::processes::{ProcessInfo, SortColumn};
use crate::tools::{BatteryInfo, MemInfo, RebootMode, StorageInfo};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Devices,
    Apps,
    Processes,
    Files,
    Logcat,
    Apk,
    Shell,
    Tools,
    Settings,
}

impl Page {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Devices => "Devices",
            Self::Apps => "Apps",
            Self::Processes => "Processes",
            Self::Files => "Files",
            Self::Logcat => "Logcat",
            Self::Apk => "APK",
            Self::Shell => "Shell",
            Self::Tools => "Device Tools",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdbStatus {
    Unknown,
    Ready,
    Unavailable,
}

pub struct AppState {
    pub config: AppConfig,
    pub saved: SavedDevices,
    pub page: Page,
    pub devices: Vec<Device>,
    pub selected_serial: Option<String>,
    /// Per-device details cache (serial → info). Fetched in background.
    pub info: HashMap<String, DeviceInfo>,
    /// Serials with an info fetch currently in flight (no duplicate workers).
    pub info_pending: HashSet<String>,
    /// Wireless-pairing state machine (Phase 3).
    pub pairing: PairingState,
    /// Last pairing detail message (success output or failure reason).
    pub pairing_message: Option<String>,
    /// Per-device app caches, keyed by serial (Phase 4).
    pub apps: HashMap<String, AppsCache>,
    /// Apps page view state (filter/search/selection/tabs/confirms).
    pub apps_view: AppsViewState,
    /// Per-device logcat buffers + crash reports, keyed by serial (Phase 5).
    pub logcat: HashMap<String, LogcatBufferState>,
    /// Per-device process caches, keyed by serial (Phase 6).
    pub processes: HashMap<String, ProcessCache>,
    /// Processes page view state (search/sort/refresh).
    pub proc_view: ProcessViewState,
    /// APK inspector state: one file at a time (Phase 7).
    pub apk: ApkState,
    /// File-browser state for the selected device (Phase 8).
    pub files: FilesState,
    /// Interactive shell state for the selected device (Phase 9).
    pub shell: ShellState,
    /// Device Tools state for the selected device (Phase 10).
    pub tools: ToolsState,
    pub adb_status: AdbStatus,
    pub adb_version: Option<String>,
    pub adb_message: String,
    pub last_error: Option<LastError>,
    pub toasts: VecDeque<ActiveToast>,
    pub confirm_delete_target: Option<String>,
    pub show_connect_dialog: bool,
    pub first_run_dismissed: bool,
    /// Command palette (Ctrl+K): open flag + query + cursor.
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_idx: usize,
}

#[derive(Debug, Clone)]
pub struct LastError {
    pub title: String,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActiveToast {
    pub toast: Toast,
    pub remaining: f32,
}

/// Per-device app data cache (Phase 4).
#[derive(Debug, Default)]
pub struct AppsCache {
    pub entries: Vec<PackageEntry>,
    pub resolved: HashMap<String, AppInfo>,
    pub running: HashSet<String>,
    pub list_loading: bool,
    pub resolving: bool,
    pub progress: (usize, usize),
}

/// Apps page view state: filter/search/selection/tabs/in-flight actions.
#[derive(Debug)]
pub struct AppsViewState {
    pub filter: AppFilter,
    pub search: String,
    pub selected: Option<String>,
    pub detail_tab: AppDetailTab,
    /// "package:action" tag of the running action, if any.
    pub busy: Option<String>,
    pub confirm: Option<PendingAppAction>,
    /// Set by AppActionDone: package whose details need re-fetching.
    pub refresh_details: Option<String>,
    /// Set by AppActionDone (uninstall): whole list needs re-fetching.
    pub refresh_list: bool,
}

impl Default for AppsViewState {
    fn default() -> Self {
        Self {
            filter: AppFilter::All,
            search: String::new(),
            selected: None,
            detail_tab: AppDetailTab::Overview,
            busy: None,
            confirm: None,
            refresh_details: None,
            refresh_list: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppDetailTab {
    #[default]
    Overview,
    Permissions,
    Activities,
    Services,
    Receivers,
    Providers,
}

impl AppDetailTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Permissions => "Permissions",
            Self::Activities => "Activities",
            Self::Services => "Services",
            Self::Receivers => "Receivers",
            Self::Providers => "Providers",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppActionKind {
    Launch,
    ForceStop,
    ClearCache,
    ClearData,
    Uninstall,
    Extract,
}

impl AppActionKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Launch => "Launch",
            Self::ForceStop => "Force Stop",
            Self::ClearCache => "Clear Cache",
            Self::ClearData => "Clear Data",
            Self::Uninstall => "Uninstall",
            Self::Extract => "Extract APK",
        }
    }

    /// Actions that require an explicit confirmation dialog.
    pub fn needs_confirm(&self) -> bool {
        matches!(self, Self::ClearData | Self::Uninstall)
    }
}

#[derive(Debug, Clone)]
pub struct PendingAppAction {
    pub kind: AppActionKind,
    pub package: String,
    pub label: String,
}

/// Per-device logcat view state: ring buffer, filters, crashes (Phase 5).
#[derive(Debug, Clone)]
pub struct LogcatBufferState {
    pub buffer: LogBuffer,
    pub pid_map: HashMap<u32, String>,
    pub filter: LogViewFilter,
    pub paused: bool,
    pub skipped_while_paused: u64,
    pub crashes: Vec<CrashReport>,
    pub selected_crash: Option<u64>,
    pub hide_system_frames: bool,
    /// Last inline notice (export result, …) shown in the footer.
    pub notice: Option<String>,
}

impl LogcatBufferState {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: LogBuffer::new(capacity),
            pid_map: HashMap::new(),
            filter: LogViewFilter::default(),
            paused: false,
            skipped_while_paused: 0,
            crashes: Vec::new(),
            selected_crash: None,
            hide_system_frames: true,
            notice: None,
        }
    }
}

/// APK inspector state: single selected file + decoded info (Phase 7).
#[derive(Debug, Default)]
pub struct ApkState {
    /// Absolute path of the inspected file (pending + loaded).
    pub path: Option<String>,
    pub info: Option<ApkInfo>,
    /// Set while a worker thread decodes the ZIP/manifest.
    pub loading: bool,
    pub error: Option<String>,
    pub tab: ApkTab,
    /// `adb install -r`: replace existing app, keep its data.
    pub reinstall: bool,
    /// Set while `adb install` runs on a worker thread.
    pub installing: bool,
    pub last_install: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApkTab {
    #[default]
    Overview,
    Manifest,
    Permissions,
    Activities,
    Services,
    Receivers,
    Files,
    Certificate,
}

impl ApkTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Manifest => "Manifest",
            Self::Permissions => "Permissions",
            Self::Activities => "Activities",
            Self::Services => "Services",
            Self::Receivers => "Receivers",
            Self::Files => "Files",
            Self::Certificate => "Certificate",
        }
    }
}

/// File-browser state: current directory + listing + pending inputs (Phase 8).
#[derive(Debug, Default)]
pub struct FilesState {
    /// Device the listing belongs to; `None` until the first fetch.
    pub serial: Option<String>,
    pub cwd: String,
    /// Directory a fetch is in flight for; stale results are dropped.
    pub pending: Option<String>,
    pub entries: Vec<FileEntry>,
    pub loading: bool,
    /// "op target" tag of the running mutation/transfer, if any.
    pub busy: Option<String>,
    pub error: Option<String>,
    pub search: String,
    pub mkdir_name: String,
    /// Remote path being renamed (inline editor target).
    pub rename_target: Option<String>,
    pub rename_new: String,
    /// Remote path armed for delete confirmation.
    pub confirm_delete: Option<String>,
    /// Set by FileOpDone (ok): the listing needs re-fetching.
    pub refresh_list: bool,
}

/// One completed shell command + its output.
#[derive(Debug, Clone)]
pub struct ShellBlock {
    pub cmd: String,
    pub output: Vec<String>,
    pub code: i32,
}

/// Interactive shell state: transcript, input, history (Phase 9).
#[derive(Debug, Default)]
pub struct ShellState {
    /// Device the session belongs to; `None` until first start.
    pub serial: Option<String>,
    pub blocks: Vec<ShellBlock>,
    pub input: String,
    pub history: Vec<String>,
    pub hist_idx: Option<usize>,
    /// Command currently executing (Send disabled until it completes).
    pub running: Option<String>,
    pub connected: bool,
    pub error: Option<String>,
    /// Inline notice (save result, …).
    pub notice: Option<String>,
}

impl ShellState {
    pub fn push_history(&mut self, cmd: String) {
        if self.history.last().is_some_and(|last| *last == cmd) {
            return;
        }
        self.history.push(cmd);
        while self.history.len() > 100 {
            self.history.remove(0);
        }
        self.hist_idx = None;
    }
}

/// Device Tools state: snapshots, screenshot, recording (Phase 10).
#[derive(Debug)]
pub struct ToolsState {
    /// Device the snapshots belong to; `None` until the first fetch.
    pub serial: Option<String>,
    pub battery: Option<BatteryInfo>,
    pub memory: Option<MemInfo>,
    pub storage: Vec<StorageInfo>,
    pub props: Vec<(String, String)>,
    pub info_loading: bool,
    pub info_error: Option<String>,
    pub props_search: String,
    pub shot_png: Option<Vec<u8>>,
    pub shot_local: Option<String>,
    pub shot_busy: bool,
    pub rec_phase: RecPhase,
    /// Chosen recording length (seconds, ≤ 180).
    pub rec_limit: u32,
    pub rec_remote: Option<String>,
    pub rec_started: Option<std::time::Instant>,
    pub rec_local: Option<String>,
    pub rec_error: Option<String>,
    /// "action target" tag of a running reboot/restart/clear.
    pub busy: Option<String>,
    pub confirm_reboot: Option<RebootMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecPhase {
    #[default]
    Idle,
    Recording,
}

impl Default for ToolsState {
    fn default() -> Self {
        Self {
            serial: None,
            battery: None,
            memory: None,
            storage: Vec::new(),
            props: Vec::new(),
            info_loading: false,
            info_error: None,
            props_search: String::new(),
            shot_png: None,
            shot_local: None,
            shot_busy: false,
            rec_phase: RecPhase::Idle,
            rec_limit: 30,
            rec_remote: None,
            rec_started: None,
            rec_local: None,
            rec_error: None,
            busy: None,
            confirm_reboot: None,
        }
    }
}

/// Per-device process list cache (Phase 6).
#[derive(Debug, Default, Clone)]
pub struct ProcessCache {
    pub procs: Vec<ProcessInfo>,
    pub loading: bool,
}

/// Processes page view state: search, sorting, refresh mode, in-flight action.
#[derive(Debug, Clone)]
pub struct ProcessViewState {
    pub search: String,
    pub sort: SortColumn,
    pub ascending: bool,
    pub auto_refresh: bool,
    pub busy: Option<String>,
    /// Set by ProcessActionDone: list needs re-fetching.
    pub refresh_list: bool,
}

impl Default for ProcessViewState {
    fn default() -> Self {
        Self {
            search: String::new(),
            sort: SortColumn::Cpu,
            ascending: false,
            auto_refresh: true,
            busy: None,
            refresh_list: false,
        }
    }
}

impl AppState {
    pub fn new(config: AppConfig, saved: SavedDevices) -> Self {
        Self {
            config,
            saved,
            page: Page::Dashboard,
            devices: Vec::new(),
            selected_serial: None,
            info: HashMap::new(),
            info_pending: HashSet::new(),
            pairing: PairingState::Idle,
            pairing_message: None,
            apps: HashMap::new(),
            apps_view: AppsViewState::default(),
            logcat: HashMap::new(),
            processes: HashMap::new(),
            proc_view: ProcessViewState::default(),
            apk: ApkState::default(),
            files: FilesState::default(),
            shell: ShellState::default(),
            tools: ToolsState::default(),
            adb_status: AdbStatus::Unknown,
            adb_version: None,
            adb_message: "Locating ADB…".to_string(),
            last_error: None,
            toasts: VecDeque::new(),
            confirm_delete_target: None,
            show_connect_dialog: false,
            first_run_dismissed: false,
            palette_open: false,
            palette_query: String::new(),
            palette_idx: 0,
        }
    }

    pub fn selected_device(&self) -> Option<&Device> {
        let serial = self.selected_serial.as_ref()?;
        self.devices.iter().find(|d| &d.serial == serial)
    }

    /// Keep selection valid after each refresh; auto-select first usable device.
    fn fix_selection(&mut self) {
        let still_present = self
            .selected_serial
            .as_ref()
            .is_some_and(|s| self.devices.iter().any(|d| &d.serial == s));
        if !still_present {
            self.selected_serial = self
                .devices
                .iter()
                .find(|d| d.state.is_usable())
                .or(self.devices.first())
                .map(|d| d.serial.clone());
        }
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::DevicesRefreshed { devices } => {
                self.devices = devices;
                // Prune cached info for devices that vanished.
                self.info
                    .retain(|serial, _| self.devices.iter().any(|d| &d.serial == serial));
                self.info_pending
                    .retain(|serial| self.devices.iter().any(|d| &d.serial == serial));
                self.fix_selection();
            }
            AppEvent::DeviceConnected { device } => {
                if !self.devices.iter().any(|d| d.serial == device.serial) {
                    self.devices.push(device);
                    self.fix_selection();
                }
            }
            AppEvent::DeviceDisconnected { serial } => {
                self.devices.retain(|d| d.serial != serial);
                self.info.remove(&serial);
                self.info_pending.remove(&serial);
                self.fix_selection();
            }
            AppEvent::DeviceStateChanged { device } => {
                if let Some(slot) = self.devices.iter_mut().find(|d| d.serial == device.serial) {
                    *slot = device;
                }
                self.fix_selection();
            }
            AppEvent::DeviceInfoUpdated { serial, info } => {
                self.info_pending.remove(&serial);
                // Drop stale results for devices that vanished mid-fetch.
                if self.devices.iter().any(|d| d.serial == serial) {
                    self.info.insert(serial, info);
                }
            }
            AppEvent::PairingResult { success, message } => {
                self.pairing = if success {
                    PairingState::Paired
                } else {
                    PairingState::Failed
                };
                self.pairing_message = Some(message.clone());
                self.push_toast(if success {
                    Toast::success(format!("Paired ({message})"))
                } else {
                    Toast::error(format!("Pairing failed: {message}"))
                });
            }
            AppEvent::AppsPackages {
                serial,
                entries,
                running,
            } => {
                let cache = self.apps.entry(serial).or_default();
                cache.entries = entries;
                cache.running = running;
                cache.list_loading = false;
            }
            AppEvent::AppsProgress {
                serial,
                done,
                total,
            } => {
                let cache = self.apps.entry(serial).or_default();
                cache.resolving = done < total;
                cache.progress = (done, total);
            }
            AppEvent::AppsResolved { serial, apps } => {
                let cache = self.apps.entry(serial).or_default();
                for app in apps {
                    cache.resolved.insert(app.package.clone(), app);
                }
                cache.resolving = false;
            }
            AppEvent::AppDetails { serial, info } => {
                let cache = self.apps.entry(serial).or_default();
                cache.resolved.insert(info.package.clone(), info);
            }
            AppEvent::AppActionDone {
                serial,
                action,
                package,
                ok,
                message,
            } => {
                self.apps_view.busy = None;
                self.apps_view.confirm = None;
                if ok {
                    // Uninstall changes the list; other actions change details.
                    if action == "Uninstall" {
                        self.apps_view.refresh_list = true;
                        if let Some(cache) = self.apps.get_mut(&serial) {
                            cache.entries.retain(|e| e.package != package);
                            cache.resolved.remove(&package);
                        }
                        if self.apps_view.selected.as_deref() == Some(&package) {
                            self.apps_view.selected = None;
                        }
                    } else {
                        self.apps_view.refresh_details = Some(package.clone());
                    }
                }
                self.push_toast(if ok {
                    Toast::success(format!("{action} {package}: {message}"))
                } else {
                    Toast::error(format!("{action} {package} failed: {message}"))
                });
                let _ = serial;
            }
            AppEvent::AppExtractDone {
                serial,
                package,
                ok,
                message,
                files,
            } => {
                self.apps_view.busy = None;
                if ok {
                    self.push_toast(Toast::success(format!(
                        "Extracted {package} ({} file{}): {message}",
                        files.len(),
                        if files.len() == 1 { "" } else { "s" },
                    )));
                } else {
                    self.push_toast(Toast::error(format!(
                        "Extracting {package} failed: {message}"
                    )));
                }
                let _ = serial;
            }
            AppEvent::LogBatch {
                serial,
                entries,
                pid_map,
            } => {
                let capacity = self.config.log_buffer_size;
                let buf = self
                    .logcat
                    .entry(serial)
                    .or_insert_with(|| LogcatBufferState::new(capacity));
                buf.buffer.set_capacity(capacity);
                buf.pid_map.extend(pid_map);
                if buf.paused {
                    buf.skipped_while_paused += entries.len() as u64;
                } else {
                    for entry in entries {
                        buf.buffer.push(entry);
                    }
                }
            }
            AppEvent::LogcatError { serial, message } => {
                self.push_toast(Toast::error(format!(
                    "Logcat ({serial}) stopped: {message}"
                )));
            }
            AppEvent::CrashDetected { serial, report } => {
                let pause = self.config.pause_on_crash;
                let buf = self
                    .logcat
                    .entry(serial.clone())
                    .or_insert_with(|| LogcatBufferState::new(self.config.log_buffer_size));
                if buf.crashes.len() >= 50 {
                    buf.crashes.remove(0);
                }
                buf.crashes.push(report.clone());
                if pause {
                    buf.paused = true;
                }
                self.push_toast(Toast::warning(format!(
                    "⚠ {} detected: {} — {}",
                    report.reason.label(),
                    report.package,
                    report.short_exception()
                )));
            }
            AppEvent::ProcessesUpdated { serial, procs } => {
                let cache = self.processes.entry(serial).or_default();
                cache.procs = procs;
                cache.loading = false;
            }
            AppEvent::ProcessActionDone {
                serial,
                action,
                target,
                ok,
                message,
            } => {
                self.proc_view.busy = None;
                if ok {
                    self.proc_view.refresh_list = true;
                }
                self.push_toast(if ok {
                    Toast::success(format!("{action} {target}: {message}"))
                } else {
                    Toast::error(format!("{action} {target} failed: {message}"))
                });
                let _ = serial;
            }
            AppEvent::ApkInspected { path, info } => {
                // Ignore stale results when the user moved on to another file.
                if self.apk.path.as_deref() == Some(path.as_str()) {
                    self.apk.info = Some(*info);
                    self.apk.error = None;
                }
                self.apk.loading = false;
            }
            AppEvent::ApkInspectFailed { path, message } => {
                if self.apk.path.as_deref() == Some(path.as_str()) {
                    self.apk.info = None;
                    self.apk.error = Some(message.clone());
                }
                self.apk.loading = false;
                self.push_toast(Toast::error(format!("APK inspection failed: {message}")));
            }
            AppEvent::ApkInstallDone {
                serial,
                files,
                ok,
                message,
            } => {
                self.apk.installing = false;
                let what = files
                    .iter()
                    .map(|f| {
                        std::path::Path::new(f)
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| f.clone())
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                self.apk.last_install = Some(message.clone());
                self.push_toast(if ok {
                    Toast::success(format!("Installed {what} on {serial}: {message}"))
                } else {
                    Toast::error(format!("Install {what} failed: {message}"))
                });
            }
            AppEvent::FilesListed {
                serial,
                dir,
                entries,
            } => {
                // Stale results (device switched / navigated mid-fetch) are dropped.
                self.files.loading = false;
                if self.files.serial.as_deref() == Some(serial.as_str())
                    && self.files.pending.as_deref() == Some(dir.as_str())
                {
                    self.files.pending = None;
                    self.files.cwd = dir;
                    self.files.entries = entries;
                    self.files.error = None;
                }
            }
            AppEvent::FilesError {
                serial,
                dir,
                message,
            } => {
                self.files.loading = false;
                if self.files.serial.as_deref() == Some(serial.as_str())
                    && self.files.pending.as_deref() == Some(dir.as_str())
                {
                    self.files.pending = None;
                    self.files.error = Some(format!("{dir}: {message}"));
                }
                self.push_toast(Toast::error(format!("Browsing {dir} failed: {message}")));
            }
            AppEvent::FileOpDone {
                serial,
                op,
                target,
                ok,
                message,
            } => {
                self.files.busy = None;
                self.files.confirm_delete = None;
                if ok {
                    self.files.refresh_list = true;
                }
                self.push_toast(if ok {
                    Toast::success(format!("{op} {target}: {message}"))
                } else {
                    Toast::error(format!("{op} {target} failed: {message}"))
                });
                let _ = serial;
            }
            AppEvent::ShellReady { serial } => {
                if self.shell.serial.as_deref() == Some(serial.as_str()) {
                    self.shell.connected = true;
                    self.shell.error = None;
                }
            }
            AppEvent::ShellOutput {
                serial,
                cmd,
                output,
                code,
            } => {
                if self.shell.serial.as_deref() == Some(serial.as_str()) {
                    self.shell.running = None;
                    self.shell.connected = true;
                    self.shell.blocks.push(ShellBlock {
                        cmd: cmd.clone(),
                        output,
                        code,
                    });
                    while self.shell.blocks.len() > 500 {
                        self.shell.blocks.remove(0);
                    }
                    self.shell.push_history(cmd);
                }
            }
            AppEvent::ShellExited { serial, message } => {
                if self.shell.serial.as_deref() == Some(serial.as_str()) {
                    self.shell.connected = false;
                    self.shell.running = None;
                }
                self.push_toast(Toast::info(message));
            }
            AppEvent::ShellError { serial, message } => {
                if self.shell.serial.as_deref() == Some(serial.as_str()) {
                    self.shell.error = Some(message.clone());
                    self.shell.connected = false;
                    self.shell.running = None;
                }
                self.push_toast(Toast::error(format!("Shell ({serial}) failed: {message}")));
            }
            AppEvent::ToolBattery { serial, info } => {
                if self.tools.serial.as_deref() == Some(serial.as_str()) {
                    self.tools.battery = Some(info);
                }
            }
            AppEvent::ToolMemory { serial, info } => {
                if self.tools.serial.as_deref() == Some(serial.as_str()) {
                    self.tools.memory = Some(info);
                }
            }
            AppEvent::ToolStorage { serial, entries } => {
                if self.tools.serial.as_deref() == Some(serial.as_str()) {
                    self.tools.storage = entries;
                }
            }
            AppEvent::ToolProps { serial, props } => {
                if self.tools.serial.as_deref() == Some(serial.as_str()) {
                    self.tools.props = props;
                }
            }
            AppEvent::ToolInfoDone { serial } => {
                if self.tools.serial.as_deref() == Some(serial.as_str()) {
                    self.tools.info_loading = false;
                }
            }
            AppEvent::ToolInfoError {
                serial,
                kind,
                message,
            } => {
                if self.tools.serial.as_deref() == Some(serial.as_str()) {
                    self.tools.info_loading = false;
                    self.tools.info_error = Some(format!("{kind}: {message}"));
                }
                self.push_toast(Toast::error(format!("{kind} failed: {message}")));
            }
            AppEvent::ScreenshotDone {
                serial,
                ok,
                message,
                local,
                png,
            } => {
                self.tools.shot_busy = false;
                if ok {
                    self.tools.shot_png = png;
                    self.tools.shot_local = local.clone();
                }
                self.push_toast(if ok {
                    Toast::success(format!(
                        "Screenshot: {message} ({})",
                        local.unwrap_or_default()
                    ))
                } else {
                    Toast::error(format!("Screenshot failed: {message}"))
                });
                let _ = serial;
            }
            AppEvent::RecordingDone {
                serial,
                ok,
                message,
                local,
            } => {
                self.tools.rec_phase = RecPhase::Idle;
                if ok {
                    self.tools.rec_local = local.clone();
                    self.tools.rec_error = None;
                } else {
                    self.tools.rec_error = Some(message.clone());
                }
                self.push_toast(if ok {
                    Toast::success(format!(
                        "Recording saved: {} ({message})",
                        local.unwrap_or_default()
                    ))
                } else {
                    Toast::error(format!("Recording failed: {message}"))
                });
                let _ = serial;
            }
            AppEvent::ToolActionDone {
                serial,
                action,
                ok,
                message,
            } => {
                self.tools.busy = None;
                self.tools.confirm_reboot = None;
                self.push_toast(if ok {
                    Toast::success(format!("{action}: {message}"))
                } else {
                    Toast::error(format!("{action} failed: {message}"))
                });
                let _ = serial;
            }
            AppEvent::AdbStatusChanged {
                available,
                version,
                message,
            } => {
                self.adb_status = if available {
                    AdbStatus::Ready
                } else {
                    AdbStatus::Unavailable
                };
                self.adb_version = version;
                self.adb_message = message;
            }
            AppEvent::Toast(toast) => {
                self.push_toast(toast);
            }
            AppEvent::Error {
                title,
                message,
                details,
            } => {
                self.last_error = Some(LastError {
                    title: title.clone(),
                    message: message.clone(),
                    details,
                });
                self.push_toast(Toast::error(format!("{title}: {message}")));
            }
        }
    }

    pub fn push_toast(&mut self, toast: Toast) {
        let remaining = toast.ttl_secs;
        self.toasts.push_back(ActiveToast { toast, remaining });
        while self.toasts.len() > 5 {
            self.toasts.pop_front();
        }
    }

    pub fn tick_toasts(&mut self, dt: f32) {
        for t in self.toasts.iter_mut() {
            t.remaining -= dt;
        }
        while self.toasts.front().is_some_and(|t| t.remaining <= 0.0) {
            self.toasts.pop_front();
        }
    }

    pub fn set_adb_error(&mut self, err: &AdbError) {
        self.last_error = Some(LastError {
            title: err.title().to_string(),
            message: err.guidance().to_string(),
            details: Some(err.to_string()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::{DeviceState, Transport};

    fn device(serial: &str) -> Device {
        Device {
            serial: serial.to_string(),
            state: DeviceState::Connected,
            raw_state: "device".to_string(),
            transport: Transport::Usb,
            model: None,
            product: None,
            device_name: None,
            transport_id: None,
            usb: None,
        }
    }

    #[test]
    fn info_cached_and_pruned_with_devices() {
        let mut state = AppState::new(AppConfig::default(), SavedDevices::default());
        state.handle_event(AppEvent::DevicesRefreshed {
            devices: vec![device("A"), device("B")],
        });
        state.info_pending.insert("A".to_string());
        state.handle_event(AppEvent::DeviceInfoUpdated {
            serial: "A".to_string(),
            info: DeviceInfo {
                android_version: Some("16".to_string()),
                ..Default::default()
            },
        });
        assert!(state.info_pending.is_empty());
        assert_eq!(state.info["A"].android_version.as_deref(), Some("16"));

        // Stale info for vanished devices is dropped on refresh.
        state.handle_event(AppEvent::DevicesRefreshed {
            devices: vec![device("B")],
        });
        assert!(!state.info.contains_key("A"));

        // Results for unknown serials are ignored.
        state.handle_event(AppEvent::DeviceInfoUpdated {
            serial: "GHOST".to_string(),
            info: DeviceInfo::default(),
        });
        assert!(!state.info.contains_key("GHOST"));
    }
}
