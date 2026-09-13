//! Local configuration: `%APPDATA%\ADBManager\config.json` on Windows.
//!
//! Never stores passwords or credentials — only the adb path and UI prefs.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub adb_path: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub auto_refresh: bool,
    #[serde(default = "default_refresh_secs")]
    pub refresh_interval_secs: u64,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default = "default_true")]
    pub remember_devices: bool,
    #[serde(default = "default_true")]
    pub confirm_destructive: bool,
    #[serde(default = "default_log_buffer")]
    pub log_buffer_size: usize,
    #[serde(default = "default_true")]
    pub log_auto_scroll: bool,
    #[serde(default = "default_true")]
    pub pause_on_crash: bool,
    /// Optional external `aapt2` executable for advanced dumps. Inspection
    /// itself is pure-Rust and never needs it; when set, the APK page offers
    /// a "Dump with aapt2" helper that shells out to this path only.
    #[serde(default)]
    pub aapt2_path: Option<PathBuf>,
    /// Default root of the file browser (`/sdcard/` on virtually every
    /// device; configurable for work profiles / exotic mounts).
    #[serde(default = "default_files_root")]
    pub files_root: String,
}

fn default_true() -> bool {
    true
}
fn default_refresh_secs() -> u64 {
    2
}
fn default_log_buffer() -> usize {
    10_000
}
fn default_files_root() -> String {
    "/sdcard".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            adb_path: None,
            auto_refresh: true,
            refresh_interval_secs: 2,
            auto_reconnect: true,
            remember_devices: true,
            confirm_destructive: true,
            log_buffer_size: 10_000,
            log_auto_scroll: true,
            pause_on_crash: true,
            aapt2_path: None,
            files_root: default_files_root(),
        }
    }
}

impl AppConfig {
    /// `%APPDATA%\ADBManager` on Windows, OS config dir elsewhere.
    pub fn app_dir() -> PathBuf {
        #[cfg(windows)]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                return PathBuf::from(appdata).join("ADBManager");
            }
        }
        if let Some(proj) = directories::ProjectDirs::from("", "", "ADBManager") {
            return proj.config_dir().to_path_buf();
        }
        // Last resort: alongside the executable's working dir.
        PathBuf::from(".").join(".adb-manager")
    }

    pub fn file_path() -> PathBuf {
        Self::app_dir().join("config.json")
    }

    pub fn log_dir() -> PathBuf {
        Self::app_dir().join("logs")
    }

    pub fn load() -> Self {
        let path = Self::file_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::app_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating config dir {}", dir.display()))?;
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(Self::file_path(), text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_through_json() {
        let cfg = AppConfig {
            adb_path: Some(PathBuf::from("C:\\platform-tools\\adb.exe")),
            ..Default::default()
        };
        let text = serde_json::to_string(&cfg).unwrap();
        let back: AppConfig = serde_json::from_str(&text).unwrap();
        assert_eq!(back.adb_path, cfg.adb_path);
        assert_eq!(back.refresh_interval_secs, 2);
        assert_eq!(back.log_buffer_size, 10_000);
    }

    #[test]
    fn missing_fields_get_defaults() {
        let back: AppConfig = serde_json::from_str("{}").unwrap();
        assert!(back.auto_refresh);
        assert_eq!(back.refresh_interval_secs, 2);
    }
}
