//! Application model + list filter.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AppFilter {
    #[default]
    All,
    User,
    System,
    Running,
}

impl AppFilter {
    pub fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::User => "User Apps",
            Self::System => "System Apps",
            Self::Running => "Running",
        }
    }
}

/// One row of `pm list packages` (fast pass; details resolved later).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEntry {
    pub package: String,
    pub system: bool,
}

/// Fully resolved application details (`dumpsys package` + `pm path`).
#[derive(Debug, Clone, Default)]
pub struct AppInfo {
    pub package: String,
    pub label: Option<String>,
    pub version_name: Option<String>,
    pub version_code: Option<String>,
    pub uid: Option<String>,
    pub installer: Option<String>,
    pub enabled: bool,
    pub system: bool,
    pub running: bool,
    pub apk_paths: Vec<String>,
    pub install_permissions: Vec<PermissionStatus>,
    pub runtime_permissions: Vec<PermissionStatus>,
    pub activities: Vec<String>,
    pub services: Vec<String>,
    pub receivers: Vec<String>,
    pub providers: Vec<String>,
    /// True when even the basic version block could not be parsed.
    pub partial: bool,
}

impl AppInfo {
    pub fn display_name(&self) -> String {
        self.label
            .clone()
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| self.package.clone())
    }

    pub fn state_label(&self) -> &'static str {
        if !self.enabled {
            "Disabled"
        } else if self.running {
            "Running"
        } else {
            "Stopped"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionStatus {
    pub name: String,
    pub granted: bool,
}

impl PermissionStatus {
    /// Short name for tables, e.g. `android.permission.CAMERA` → `CAMERA`.
    pub fn short_name(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or(&self.name)
    }
}
