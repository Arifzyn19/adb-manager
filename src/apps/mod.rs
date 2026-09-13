//! Installed-application management (Phase 4).
//!
//! - [`package`]: app model + list filter.
//! - [`parser`]: `pm` / `dumpsys package` / `ps` output parsers.
//! - [`manager`]: fetch + action orchestration (always via [`AdbClient`]).

pub mod manager;
pub mod package;
pub mod parser;

pub use manager::{
    apk_paths, clear_app, fetch_app_details, fetch_package_entries, fetch_running_set,
    force_stop_app, launch_app, pull_apk, uninstall_app,
};
pub use package::{AppFilter, AppInfo, PackageEntry, PermissionStatus};
