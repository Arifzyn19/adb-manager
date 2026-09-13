//! One-shot + looping device polling helpers.
//!
//! Runs on a background thread; never blocks the egui UI thread.

use crate::adb::{AdbClient, Device};
use std::path::PathBuf;

/// Mock mode for UI development without hardware:
/// `ADB_MANAGER_MOCK=1 cargo run` yields fake devices + fake adb version.
pub fn is_mock() -> bool {
    std::env::var("ADB_MANAGER_MOCK")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Run `adb devices -l` once. Returns an empty vec if adb is missing so the
/// polling loop can keep retrying after the user configures a path.
pub fn poll_devices_once(adb_path: &PathBuf) -> Vec<Device> {
    if is_mock() {
        return mock_devices();
    }
    let client = AdbClient::new(adb_path.clone());
    client.devices().unwrap_or_default()
}

pub fn mock_devices() -> Vec<Device> {
    use crate::adb::{DeviceState, Transport};
    vec![
        Device {
            serial: "0A201JEC200123".to_string(),
            state: DeviceState::Connected,
            raw_state: "device".to_string(),
            transport: Transport::Usb,
            model: Some("vivo_I2219".to_string()),
            product: Some("I2219".to_string()),
            device_name: Some("I2219".to_string()),
            transport_id: Some("2".to_string()),
            usb: Some("1-1".to_string()),
        },
        Device {
            serial: "192.168.1.20:39001".to_string(),
            state: DeviceState::Connected,
            raw_state: "device".to_string(),
            transport: Transport::Wireless,
            model: Some("Pixel_8".to_string()),
            product: Some("oriole".to_string()),
            device_name: Some("oriole".to_string()),
            transport_id: Some("4".to_string()),
            usb: None,
        },
        Device {
            serial: "ABCD1234".to_string(),
            state: DeviceState::Unauthorized,
            raw_state: "unauthorized".to_string(),
            transport: Transport::Usb,
            model: None,
            product: None,
            device_name: None,
            transport_id: Some("5".to_string()),
            usb: None,
        },
    ]
}
