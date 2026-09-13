//! Background device manager: polls ADB, diffs snapshots, emits events.

use crate::adb::Device;
use crate::device::discovery::poll_devices_once;
use crate::events::{AppEvent, Toast};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

/// Poll loop handle owned by [`crate::app::AdbManagerApp`].
pub struct DeviceManager {
    adb_path: PathBuf,
    interval: Duration,
    events: Sender<AppEvent>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl DeviceManager {
    pub fn new(adb_path: PathBuf, interval_secs: u64, events: Sender<AppEvent>) -> Self {
        Self {
            adb_path,
            interval: Duration::from_secs(interval_secs.clamp(1, 60)),
            events,
            stop: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            worker: None,
        }
    }

    /// (Re)start the background polling thread.
    pub fn start(&mut self, adb_path: PathBuf, interval_secs: u64) {
        self.stop();
        self.adb_path = adb_path;
        self.interval = Duration::from_secs(interval_secs.clamp(1, 60));
        self.stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let path = self.adb_path.clone();
        let interval = self.interval;
        let tx = self.events.clone();
        let stop = self.stop.clone();

        self.worker = Some(std::thread::spawn(move || {
            let mut known: HashMap<String, Device> = HashMap::new();
            let mut first_run = true;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let devices = poll_devices_once(&path);
                diff_and_emit(&mut known, &devices, &tx, first_run);
                first_run = false;
                // Sleep in small slices so `stop()` reacts quickly.
                let slices = (interval.as_millis() / 100).max(1) as u64;
                for _ in 0..slices {
                    if stop.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }));
    }

    pub fn stop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

impl Drop for DeviceManager {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Compare `known` with a fresh snapshot and emit granular events.
/// This pure function is unit-tested without threads.
pub fn diff_and_emit(
    known: &mut HashMap<String, Device>,
    current: &[Device],
    tx: &Sender<AppEvent>,
    silent_first_run: bool,
) {
    let mut current_map: HashMap<String, Device> = HashMap::new();
    for d in current {
        current_map.insert(d.serial.clone(), d.clone());
    }

    // Always refresh the list view.
    let _ = tx.send(AppEvent::DevicesRefreshed {
        devices: current.to_vec(),
    });

    if silent_first_run {
        *known = current_map;
        return;
    }

    for (serial, device) in &current_map {
        match known.get(serial) {
            None => {
                let _ = tx.send(AppEvent::DeviceConnected {
                    device: device.clone(),
                });
                if device.state.is_usable() {
                    let _ = tx.send(AppEvent::Toast(Toast::success(format!(
                        "Device connected: {}",
                        device.display_name()
                    ))));
                } else {
                    let _ = tx.send(AppEvent::Toast(Toast::warning(format!(
                        "Device {} is {}",
                        device.display_name(),
                        device.state.label()
                    ))));
                }
            }
            Some(prev) if prev.state != device.state => {
                let _ = tx.send(AppEvent::DeviceStateChanged {
                    device: device.clone(),
                });
            }
            _ => {}
        }
    }

    for serial in known.keys() {
        if !current_map.contains_key(serial) {
            let _ = tx.send(AppEvent::DeviceDisconnected {
                serial: serial.clone(),
            });
            let _ = tx.send(AppEvent::Toast(Toast::warning(format!(
                "Device disconnected: {serial}"
            ))));
        }
    }

    *known = current_map;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::{DeviceState, Transport};

    fn dev(serial: &str, state: DeviceState) -> Device {
        Device {
            serial: serial.to_string(),
            state,
            raw_state: "device".to_string(),
            transport: Transport::Usb,
            model: Some("Test".to_string()),
            product: None,
            device_name: None,
            transport_id: None,
            usb: None,
        }
    }

    #[test]
    fn diff_emits_connect_and_disconnect() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut known = HashMap::new();

        diff_and_emit(&mut known, &[dev("A", DeviceState::Connected)], &tx, true);
        // First run is silent apart from DevicesRefreshed.
        assert!(matches!(
            rx.recv().unwrap(),
            AppEvent::DevicesRefreshed { .. }
        ));
        assert!(rx.try_recv().is_err());

        diff_and_emit(
            &mut known,
            &[
                dev("A", DeviceState::Connected),
                dev("B", DeviceState::Connected),
            ],
            &tx,
            false,
        );
        let mut saw_connect_b = false;
        for _ in 0..3 {
            match rx.recv().unwrap() {
                AppEvent::DeviceConnected { device } if device.serial == "B" => {
                    saw_connect_b = true
                }
                _ => {}
            }
        }
        assert!(saw_connect_b);

        diff_and_emit(&mut known, &[dev("B", DeviceState::Connected)], &tx, false);
        let mut saw_disconnect_a = false;
        for _ in 0..3 {
            match rx.recv().unwrap() {
                AppEvent::DeviceDisconnected { serial } if serial == "A" => saw_disconnect_a = true,
                _ => {}
            }
        }
        assert!(saw_disconnect_a);
    }
}
