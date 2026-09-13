//! Device-tool orchestration (Phase 10). Subprocess work goes through
//! [`AdbClient`]; callers run these on worker threads and report via events.

use super::system::{
    looks_like_png, parse_df, parse_dumpsys_battery, parse_meminfo, BatteryInfo, MemInfo,
    RebootMode, StorageInfo,
};
use crate::adb::{AdbClient, AdbError};
use std::collections::HashMap;

/// `dumpsys battery` → snapshot (graceful when fields are missing).
pub fn fetch_battery(client: &AdbClient, serial: &str) -> Result<BatteryInfo, AdbError> {
    if crate::device::discovery::is_mock() {
        return Ok(BatteryInfo {
            level_pct: Some(78),
            status: super::system::BatteryStatus::Charging,
            health: super::system::BatteryHealth::Good,
            temp_c: Some(28.5),
            voltage_mv: Some(4200),
            technology: Some("Li-ion".to_string()),
            powered: vec!["USB".to_string()],
        });
    }
    Ok(parse_dumpsys_battery(
        &client.shell_text(serial, &["dumpsys", "battery"])?,
    ))
}

/// `/proc/meminfo` → snapshot.
pub fn fetch_memory(client: &AdbClient, serial: &str) -> Result<MemInfo, AdbError> {
    if crate::device::discovery::is_mock() {
        return Ok(MemInfo {
            total_kb: 8_000_000,
            avail_kb: 3_200_000,
        });
    }
    Ok(parse_meminfo(
        &client.shell_text(serial, &["cat", "/proc/meminfo"])?,
    ))
}

/// `df -h /data /sdcard` (plain-`df` fallback) → rows.
pub fn fetch_storage(client: &AdbClient, serial: &str) -> Result<Vec<StorageInfo>, AdbError> {
    if crate::device::discovery::is_mock() {
        return Ok(vec![StorageInfo {
            filesystem: "/dev/block/dm-5".to_string(),
            total_bytes: 128 * 1024u64.pow(3),
            used_bytes: 71 * 1024u64.pow(3),
            avail_bytes: 57 * 1024u64.pow(3),
            use_pct: Some(55),
            mount: "/sdcard".to_string(),
        }]);
    }
    let human = client
        .shell_text(serial, &["df", "-h", "/data", "/sdcard"])
        .unwrap_or_default();
    let rows = parse_df(&human);
    if !rows.is_empty() {
        return Ok(rows);
    }
    // Older toybox builds reject `-h` for these paths; plain `df` (1K blocks).
    let plain = client.shell_text(serial, &["df", "/data", "/sdcard"])?;
    let rows = parse_df(&plain);
    if rows.is_empty() {
        return Err(AdbError::ExecutionFailed {
            message: "Could not read storage info on this device.".to_string(),
            exit_code: None,
            stdout: plain,
            stderr: String::new(),
        });
    }
    Ok(rows)
}

/// Full `getprop` dump, sorted by key (searchable in the UI).
pub fn fetch_properties(
    client: &AdbClient,
    serial: &str,
) -> Result<Vec<(String, String)>, AdbError> {
    if crate::device::discovery::is_mock() {
        return Ok(vec![
            ("ro.product.manufacturer".to_string(), "Example".to_string()),
            ("ro.build.version.release".to_string(), "16".to_string()),
            ("ro.build.version.sdk".to_string(), "36".to_string()),
        ]);
    }
    let dump = client.shell_text(serial, &["getprop"])?;
    let mut props: Vec<(String, String)> = crate::device::info::parse_getprop(&dump)
        .into_iter()
        .collect();
    if props.is_empty() {
        return Err(AdbError::ExecutionFailed {
            message: "Could not read device properties.".to_string(),
            exit_code: None,
            stdout: dump,
            stderr: String::new(),
        });
    }
    props.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(props)
}

/// `exec-out screencap -p` → PNG bytes (validated, not just trusted).
pub fn take_screenshot(client: &AdbClient, serial: &str) -> Result<Vec<u8>, AdbError> {
    if crate::device::discovery::is_mock() {
        return Ok(mock_png());
    }
    let bytes = client.screencap(serial)?;
    if !looks_like_png(&bytes) {
        return Err(AdbError::ExecutionFailed {
            message: "screencap did not return a PNG image.".to_string(),
            exit_code: None,
            stdout: format!("{} bytes", bytes.len()),
            stderr: String::new(),
        });
    }
    Ok(bytes)
}

/// Blocking `screenrecord` run (worker thread). Returns when the time limit
/// elapses or the process is interrupted via [`stop_recording`].
pub fn run_recording(
    client: &AdbClient,
    serial: &str,
    secs: u32,
    remote: &str,
) -> Result<String, AdbError> {
    let secs = secs.clamp(1, 180);
    client.screenrecord(serial, secs, remote)
}

/// Best-effort graceful stop: SIGINT lets `screenrecord` finalize the MP4.
/// Falls back to an explanatory error when the device lacks `pkill`.
pub fn stop_recording(client: &AdbClient, serial: &str) -> Result<String, AdbError> {
    client.interrupt_screenrecord(serial)
}

/// Pull a finished recording to Windows.
pub fn pull_recording(
    client: &AdbClient,
    serial: &str,
    remote: &str,
    local: &str,
) -> Result<String, AdbError> {
    client.pull(serial, remote, local)
}

/// Delete the on-device recording scratch file (best-effort cleanup).
pub fn cleanup_recording(client: &AdbClient, serial: &str, remote: &str) {
    let _ = client.remove(serial, remote);
}

pub fn reboot_device(
    client: &AdbClient,
    serial: &str,
    mode: RebootMode,
) -> Result<String, AdbError> {
    client.reboot(serial, mode.arg())?;
    Ok(format!("{} issued for {serial}", mode.label()))
}

pub fn restart_adb(client: &AdbClient) -> Result<String, AdbError> {
    client.restart_server()
}

pub fn clear_logcat(client: &AdbClient, serial: &str) -> Result<String, AdbError> {
    client.clear_logcat(serial)
}

/// Sorted property map for tests/inspection helpers.
pub fn sorted_props(map: HashMap<String, String>) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = map.into_iter().collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// 1×1 transparent PNG for mock screenshots.
fn mock_png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_props_orders_by_key() {
        let map: HashMap<String, String> = [
            ("b".to_string(), "2".to_string()),
            ("a".to_string(), "1".to_string()),
        ]
        .into_iter()
        .collect();
        let v = sorted_props(map);
        assert_eq!(v[0].0, "a");
    }

    #[test]
    fn mock_png_passes_validation() {
        assert!(looks_like_png(&mock_png()));
    }
}
