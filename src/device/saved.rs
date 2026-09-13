//! Saved / known wireless devices (`devices.json`).
//!
//! Phase 2: remember successfully connected wireless devices and offer
//! auto-reconnect on startup. Never stores credentials — only `IP:PORT`
//! serials plus display metadata.

use crate::adb::Transport;
use crate::config::AppConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedDevices {
    #[serde(default)]
    pub devices: Vec<SavedDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedDevice {
    pub serial: String,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub transport: Transport,
    #[serde(default)]
    pub last_seen: Option<String>,
}

impl SavedDevices {
    fn file_path() -> std::path::PathBuf {
        AppConfig::app_dir().join("devices.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::file_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let dir = AppConfig::app_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::file_path(), text);
        }
    }

    pub fn contains(&self, serial: &str) -> bool {
        self.devices.iter().any(|d| d.serial == serial)
    }

    /// Insert or refresh an entry. Returns true when the list changed.
    pub fn upsert(&mut self, device: SavedDevice) -> bool {
        match self.devices.iter_mut().find(|d| d.serial == device.serial) {
            Some(slot) => {
                let changed = slot.nickname != device.nickname
                    || slot.transport != device.transport
                    || slot.last_seen != device.last_seen;
                *slot = device;
                if changed {
                    self.save();
                }
                changed
            }
            None => {
                self.devices.push(device);
                self.save();
                true
            }
        }
    }

    /// Remember a connected wireless device (USB serials are re-enumerated
    /// automatically and need no entry).
    pub fn remember_wireless(&mut self, serial: &str, nickname: Option<String>) {
        self.upsert(SavedDevice {
            serial: serial.to_string(),
            nickname,
            transport: Transport::Wireless,
            last_seen: Some(current_timestamp()),
        });
    }

    pub fn forget(&mut self, serial: &str) {
        self.devices.retain(|d| d.serial != serial);
        self.save();
    }

    /// Serials eligible for auto-reconnect: wireless entries not currently
    /// connected.
    pub fn reconnect_candidates(&self, connected: &[String]) -> Vec<String> {
        self.devices
            .iter()
            .filter(|d| d.transport == Transport::Wireless && !connected.contains(&d.serial))
            .map(|d| d.serial.clone())
            .collect()
    }
}

fn current_timestamp() -> String {
    // Seconds since epoch — opaque display string, no chrono dependency.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_dedupes_by_serial() {
        let mut saved = SavedDevices::default();
        saved.upsert(SavedDevice {
            serial: "1.2.3.4:5555".to_string(),
            nickname: None,
            transport: Transport::Wireless,
            last_seen: None,
        });
        saved.upsert(SavedDevice {
            serial: "1.2.3.4:5555".to_string(),
            nickname: Some("Pixel".to_string()),
            transport: Transport::Wireless,
            last_seen: None,
        });
        assert_eq!(saved.devices.len(), 1);
        assert_eq!(saved.devices[0].nickname.as_deref(), Some("Pixel"));
    }

    #[test]
    fn reconnect_candidates_skip_connected() {
        let mut saved = SavedDevices::default();
        saved.remember_wireless("1.2.3.4:5555", None);
        saved.upsert(SavedDevice {
            serial: "USB123".to_string(),
            nickname: None,
            transport: Transport::Usb,
            last_seen: None,
        });
        let cands = saved.reconnect_candidates(&["1.2.3.4:5555".to_string()]);
        assert!(cands.is_empty());
        let cands = saved.reconnect_candidates(&[]);
        assert_eq!(cands, vec!["1.2.3.4:5555".to_string()]);
    }

    #[test]
    fn old_files_without_new_fields_still_load() {
        let legacy = r#"{"devices":[{"serial":"1.2.3.4:5555"}]}"#;
        let saved: SavedDevices = serde_json::from_str(legacy).unwrap();
        assert_eq!(saved.devices.len(), 1);
        assert_eq!(saved.devices[0].transport, Transport::Unknown);
    }
}
