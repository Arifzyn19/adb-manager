//! tools feature module (Phase 10).
//!
//! Device Tools page backend: battery / memory / storage / property
//! snapshots (`system`), screenshots and screen recordings, reboots and ADB
//! maintenance (`manager`). All subprocess work via `AdbClient` on worker
//! threads.

pub mod manager;
pub mod system;

pub use manager::{
    cleanup_recording, clear_logcat, fetch_battery, fetch_memory, fetch_properties, fetch_storage,
    pull_recording, reboot_device, restart_adb, run_recording, stop_recording, take_screenshot,
};
pub use system::recording_remote_path;
pub use system::{BatteryInfo, MemInfo, RebootMode, StorageInfo};
