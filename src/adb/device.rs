//! Central Android device model.
//!
//! All device state in the app flows through these strongly typed structs.
//! Never assume a single device: every operation takes an explicit serial.

use serde::{Deserialize, Serialize};
use std::fmt;

/// High-level connection state of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DeviceState {
    /// `device` — fully usable.
    #[default]
    Connected,
    Unauthorized,
    Offline,
    Disconnected,
    Connecting,
    Pairing,
    Error,
    /// Any other raw state string reported by ADB (e.g. `recovery`, `sideload`).
    Unknown,
}

impl DeviceState {
    pub fn from_adb_token(token: &str) -> Self {
        match token {
            "device" => Self::Connected,
            "unauthorized" => Self::Unauthorized,
            "offline" => Self::Offline,
            "connecting" => Self::Connecting,
            _ => Self::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Unauthorized => "Unauthorized",
            Self::Offline => "Offline",
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting",
            Self::Pairing => "Pairing",
            Self::Error => "Error",
            Self::Unknown => "Unknown",
        }
    }

    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

impl fmt::Display for DeviceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Physical/logical transport of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Transport {
    #[default]
    Unknown,
    Usb,
    Wireless,
    Emulator,
}

impl Transport {
    /// Heuristic based on the serial only. Refined later with `transport_id`
    /// / model info when available.
    pub fn infer_from_serial(serial: &str) -> Self {
        if serial.starts_with("emulator-") {
            Self::Emulator
        } else if is_ip_port_serial(serial) {
            Self::Wireless
        } else {
            // USB serials and USB transport-ids land here by default.
            // A wireless device that reports a bare serial is still usable;
            // transport is best-effort display info only.
            Self::Usb
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Usb => "USB",
            Self::Wireless => "Wireless",
            Self::Emulator => "Emulator",
            Self::Unknown => "Unknown",
        }
    }
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

fn is_ip_port_serial(serial: &str) -> bool {
    // Matches "192.168.1.10:37521" style serials.
    let Some((host, port)) = serial.rsplit_once(':') else {
        return false;
    };
    if port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    host.contains('.') || host.contains(':')
}

/// A single Android device as reported by `adb devices -l`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// The ADB serial (first column of `adb devices -l`).
    pub serial: String,
    pub state: DeviceState,
    /// Raw state token, preserved for unknown states like `recovery`.
    pub raw_state: String,
    pub transport: Transport,
    pub model: Option<String>,
    pub product: Option<String>,
    pub device_name: Option<String>,
    pub transport_id: Option<String>,
    pub usb: Option<String>,
}

impl Device {
    pub fn display_name(&self) -> String {
        let model = self
            .model
            .clone()
            .unwrap_or_else(|| self.device_name.clone().unwrap_or_default());
        if model.is_empty() {
            self.serial.clone()
        } else {
            // Underscores from ADB (`vivo_I2219`) read better as spaces.
            model.replace('_', " ")
        }
    }

    pub fn short_summary(&self) -> String {
        format!(
            "{} — {} ({})",
            self.display_name(),
            self.state.label(),
            self.transport.label()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_inference() {
        assert_eq!(
            Transport::infer_from_serial("emulator-5554"),
            Transport::Emulator
        );
        assert_eq!(
            Transport::infer_from_serial("192.168.1.10:39001"),
            Transport::Wireless
        );
        assert_eq!(
            Transport::infer_from_serial("0A201JEC200123"),
            Transport::Usb
        );
    }
}
