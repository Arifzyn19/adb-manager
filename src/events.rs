//! Background-worker → UI events.
//!
//! Workers never touch egui state directly; they send [`AppEvent`]s over a
//! channel. The UI drains the channel once per frame.

use crate::adb::Device;
use crate::apk::ApkInfo;
use crate::apps::{AppInfo, PackageEntry};
use crate::device::DeviceInfo;
use crate::files::FileEntry;
use crate::logcat::{CrashReport, LogEntry};
use crate::processes::ProcessInfo;
use crate::tools::{BatteryInfo, MemInfo, StorageInfo};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub enum AppEvent {
    DevicesRefreshed {
        devices: Vec<Device>,
    },
    DeviceConnected {
        device: Device,
    },
    DeviceDisconnected {
        serial: String,
    },
    DeviceStateChanged {
        device: Device,
    },
    DeviceInfoUpdated {
        serial: String,
        info: DeviceInfo,
    },
    PairingResult {
        success: bool,
        message: String,
    },
    AppsPackages {
        serial: String,
        entries: Vec<PackageEntry>,
        running: HashSet<String>,
    },
    AppsProgress {
        serial: String,
        done: usize,
        total: usize,
    },
    AppsResolved {
        serial: String,
        apps: Vec<AppInfo>,
    },
    AppDetails {
        serial: String,
        info: AppInfo,
    },
    AppActionDone {
        serial: String,
        action: String,
        package: String,
        ok: bool,
        message: String,
    },
    AppExtractDone {
        serial: String,
        package: String,
        ok: bool,
        message: String,
        files: Vec<String>,
    },
    LogBatch {
        serial: String,
        entries: Vec<LogEntry>,
        pid_map: HashMap<u32, String>,
    },
    LogcatError {
        serial: String,
        message: String,
    },
    CrashDetected {
        serial: String,
        report: CrashReport,
    },
    ProcessesUpdated {
        serial: String,
        procs: Vec<ProcessInfo>,
    },
    ProcessActionDone {
        serial: String,
        action: String,
        target: String,
        ok: bool,
        message: String,
    },
    ApkInspected {
        path: String,
        info: Box<ApkInfo>,
    },
    ApkInspectFailed {
        path: String,
        message: String,
    },
    ApkInstallDone {
        serial: String,
        files: Vec<String>,
        ok: bool,
        message: String,
    },
    FilesListed {
        serial: String,
        dir: String,
        entries: Vec<FileEntry>,
    },
    FilesError {
        serial: String,
        dir: String,
        message: String,
    },
    FileOpDone {
        serial: String,
        op: String,
        target: String,
        ok: bool,
        message: String,
    },
    ShellReady {
        serial: String,
    },
    ShellOutput {
        serial: String,
        cmd: String,
        output: Vec<String>,
        code: i32,
    },
    ShellExited {
        serial: String,
        message: String,
    },
    ShellError {
        serial: String,
        message: String,
    },
    ToolBattery {
        serial: String,
        info: BatteryInfo,
    },
    ToolMemory {
        serial: String,
        info: MemInfo,
    },
    ToolStorage {
        serial: String,
        entries: Vec<StorageInfo>,
    },
    ToolProps {
        serial: String,
        props: Vec<(String, String)>,
    },
    ToolInfoDone {
        serial: String,
    },
    ToolInfoError {
        serial: String,
        kind: String,
        message: String,
    },
    ScreenshotDone {
        serial: String,
        ok: bool,
        message: String,
        local: Option<String>,
        png: Option<Vec<u8>>,
    },
    RecordingDone {
        serial: String,
        ok: bool,
        message: String,
        local: Option<String>,
    },
    ToolActionDone {
        serial: String,
        action: String,
        ok: bool,
        message: String,
    },
    AdbStatusChanged {
        available: bool,
        version: Option<String>,
        message: String,
    },
    Toast(Toast),
    Error {
        title: String,
        message: String,
        details: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub message: String,
    /// Seconds until auto-dismiss.
    pub ttl_secs: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Warning,
    Error,
}

impl Toast {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            kind: ToastKind::Success,
            message: message.into(),
            ttl_secs: 4.0,
        }
    }
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            kind: ToastKind::Info,
            message: message.into(),
            ttl_secs: 4.0,
        }
    }
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            kind: ToastKind::Warning,
            message: message.into(),
            ttl_secs: 6.0,
        }
    }
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            kind: ToastKind::Error,
            message: message.into(),
            ttl_secs: 8.0,
        }
    }
}
