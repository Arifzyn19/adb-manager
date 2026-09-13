//! Device discovery worker + per-device info + saved-device store.

pub mod discovery;
pub mod info;
pub mod manager;
pub mod saved;
pub mod wireless;

pub use info::{fetch_info, DeviceInfo};
pub use manager::DeviceManager;
pub use saved::SavedDevices;
