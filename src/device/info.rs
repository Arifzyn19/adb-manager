//! Per-device detailed info via `getprop` + `wm size`/`wm density`.
//!
//! All shell output parsing is defensive: any missing/odd property yields
//! `None` (shown as "—") instead of an error, because property sets vary
//! across Android versions and manufacturers.

use crate::adb::AdbClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub manufacturer: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub device_name: Option<String>,
    pub product: Option<String>,
    pub android_version: Option<String>,
    pub sdk_version: Option<String>,
    pub build_id: Option<String>,
    pub fingerprint: Option<String>,
    pub architecture: Option<String>,
    pub screen_resolution: Option<String>,
    pub density: Option<String>,
    /// True when the fetch itself failed (device went away mid-query, …).
    /// Partial data may still be present.
    #[serde(default)]
    pub fetch_failed: bool,
}

impl DeviceInfo {
    /// One-line summary for cards, e.g. "Android 16 · API 36 · arm64-v8a".
    pub fn system_summary(&self) -> String {
        let parts = [
            self.android_version
                .as_deref()
                .map(|v| format!("Android {v}")),
            self.sdk_version.as_deref().map(|v| format!("API {v}")),
            self.architecture.clone(),
        ];
        let joined = parts.into_iter().flatten().collect::<Vec<_>>().join(" · ");
        if joined.is_empty() {
            "Unknown".to_string()
        } else {
            joined
        }
    }

    pub fn get(&self, label: &str) -> &str {
        match label {
            "Manufacturer" => self.manufacturer.as_deref().unwrap_or("—"),
            "Brand" => self.brand.as_deref().unwrap_or("—"),
            "Model" => self.model.as_deref().unwrap_or("—"),
            "Device" => self.device_name.as_deref().unwrap_or("—"),
            "Product" => self.product.as_deref().unwrap_or("—"),
            "Android" => self.android_version.as_deref().unwrap_or("—"),
            "SDK" => self.sdk_version.as_deref().unwrap_or("—"),
            "Build" => self.build_id.as_deref().unwrap_or("—"),
            "Fingerprint" => self.fingerprint.as_deref().unwrap_or("—"),
            "ABI" => self.architecture.as_deref().unwrap_or("—"),
            "Resolution" => self.screen_resolution.as_deref().unwrap_or("—"),
            "Density" => self.density.as_deref().unwrap_or("—"),
            _ => "—",
        }
    }
}

/// Parse `adb shell getprop` output (`[key]: [value]` lines) into a map.
pub fn parse_getprop(output: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in output.lines() {
        let line = line.trim();
        // Canonical form: `[ro.product.manufacturer]: [vivo]`
        if !line.starts_with('[') {
            continue;
        }
        let Some(rest) = line.strip_prefix('[') else {
            continue;
        };
        let Some((key, rest)) = rest.split_once("]:") else {
            continue;
        };
        // Rest is ` [value]`; value itself may contain `]` so only strip
        // one outer pair of brackets.
        let mut value = rest.trim_start();
        value = value.strip_prefix('[').unwrap_or(value);
        value = value.strip_suffix(']').unwrap_or(value);
        if !key.is_empty() {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

/// Parse `adb shell wm size` → e.g. `Some("1080x2400")`.
/// Prefers the physical size over an override.
pub fn parse_wm_size(output: &str) -> Option<String> {
    let mut fallback = None;
    for line in output.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Physical size:") {
            let v = v.trim();
            if is_resolution(v) {
                return Some(v.to_string());
            }
        } else if line.starts_with("Override size:") {
            let v = line["Override size:".len()..].trim();
            if is_resolution(v) {
                fallback = Some(v.to_string());
            }
        } else if is_resolution(line) {
            // Some builds print a bare `1080x2400`.
            fallback = Some(line.to_string());
        }
    }
    fallback
}

fn is_resolution(s: &str) -> bool {
    let Some((w, h)) = s.split_once('x') else {
        return false;
    };
    !w.is_empty()
        && !h.is_empty()
        && w.chars().all(|c| c.is_ascii_digit())
        && h.chars().all(|c| c.is_ascii_digit())
}

/// Parse `adb shell wm density` → e.g. `Some("420")`.
pub fn parse_wm_density(output: &str) -> Option<String> {
    for line in output.lines() {
        let line = line.trim();
        for prefix in ["Physical density:", "Override density:"] {
            if let Some(v) = line.strip_prefix(prefix) {
                let v = v.trim();
                if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn build_info(
    props: &HashMap<String, String>,
    size: Option<String>,
    density: Option<String>,
) -> DeviceInfo {
    let g = |k: &str| props.get(k).cloned().filter(|v| !v.is_empty());
    DeviceInfo {
        manufacturer: g("ro.product.manufacturer"),
        brand: g("ro.product.brand"),
        // Shell props use underscores; display with spaces at the UI layer.
        model: g("ro.product.model"),
        device_name: g("ro.product.device"),
        product: g("ro.product.name"),
        android_version: g("ro.build.version.release"),
        sdk_version: g("ro.build.version.sdk"),
        build_id: g("ro.build.id"),
        fingerprint: g("ro.build.fingerprint"),
        architecture: g("ro.product.cpu.abi"),
        screen_resolution: size,
        density,
        fetch_failed: false,
    }
}

/// Fetch all info for one device. Best-effort per query: a failing `wm`
/// call still yields the getprop data. Only hard-fails when even getprop
/// fails (device unauthorized / gone) — then `fetch_failed` is set.
pub fn fetch_info(adb_path: &PathBuf, serial: &str) -> DeviceInfo {
    if crate::device::discovery::is_mock() {
        return mock_info();
    }
    let client = AdbClient::new(adb_path.clone());
    let props = match client.shell_text(serial, &["getprop"]) {
        Ok(text) => parse_getprop(&text),
        Err(_) => {
            return DeviceInfo {
                fetch_failed: true,
                ..Default::default()
            };
        }
    };
    let size = client
        .shell_text(serial, &["wm", "size"])
        .ok()
        .and_then(|t| parse_wm_size(&t));
    let density = client
        .shell_text(serial, &["wm", "density"])
        .ok()
        .and_then(|t| parse_wm_density(&t));
    build_info(&props, size, density)
}

pub fn mock_info() -> DeviceInfo {
    DeviceInfo {
        manufacturer: Some("vivo".to_string()),
        brand: Some("vivo".to_string()),
        model: Some("vivo I2219".to_string()),
        device_name: Some("I2219".to_string()),
        product: Some("I2219".to_string()),
        android_version: Some("16".to_string()),
        sdk_version: Some("36".to_string()),
        build_id: Some("UP1A.231005.007".to_string()),
        fingerprint: Some("vivo/I2219/I2219:16/UP1A.231005.007:user/release-keys".to_string()),
        architecture: Some("arm64-v8a".to_string()),
        screen_resolution: Some("1080x2408".to_string()),
        density: Some("440".to_string()),
        fetch_failed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GETPROP_SAMPLE: &str =
        "[ro.build.fingerprint]: [vivo/I2219/I2219:16/UP1A:user/release-keys]\n\
        [ro.build.id]: [UP1A.231005.007]\n\
        [ro.build.version.release]: [16]\n\
        [ro.build.version.sdk]: [36]\n\
        [ro.product.brand]: [vivo]\n\
        [ro.product.cpu.abi]: [arm64-v8a]\n\
        [ro.product.device]: [I2219]\n\
        [ro.product.manufacturer]: [vivo]\n\
        [ro.product.model]: [V2219]\n\
        [ro.product.name]: [I2219]\n\
        [sys.boot_completed]: [1]\n\
        garbage line without brackets\n\
        [odd]: []\n";

    #[test]
    fn parses_getprop_dump() {
        let map = parse_getprop(GETPROP_SAMPLE);
        assert_eq!(map.get("ro.product.manufacturer").unwrap(), "vivo");
        assert_eq!(map.get("ro.build.version.release").unwrap(), "16");
        assert_eq!(map.get("ro.build.version.sdk").unwrap(), "36");
        assert_eq!(map.get("ro.product.cpu.abi").unwrap(), "arm64-v8a");
        // Fingerprint value itself contains no brackets, but a value with
        // brackets must survive outer-strip only.
        assert!(map
            .get("ro.build.fingerprint")
            .unwrap()
            .starts_with("vivo/"));
        assert!(!map.contains_key("garbage line without brackets"));
        assert_eq!(map.get("odd").unwrap(), "");
    }

    #[test]
    fn fingerprint_with_brackets_keeps_inner() {
        let map = parse_getprop("[k]: [a[b]c]\n");
        assert_eq!(map.get("k").unwrap(), "a[b]c");
    }

    #[test]
    fn parses_wm_outputs() {
        assert_eq!(
            parse_wm_size("Physical size: 1080x2400\nOverride size: 720x1600\n").as_deref(),
            Some("1080x2400")
        );
        assert_eq!(parse_wm_size("1080x2400\n").as_deref(), Some("1080x2400"));
        assert_eq!(parse_wm_size("Physical size: unknown\n"), None);
        assert_eq!(
            parse_wm_density("Physical density: 420\n").as_deref(),
            Some("420")
        );
        assert_eq!(
            parse_wm_density("Physical density: 420\nOverride density: 320\n").as_deref(),
            Some("420")
        );
        assert_eq!(parse_wm_density("nope\n"), None);
    }

    #[test]
    fn builds_info_with_missing_fields_as_none() {
        let map = parse_getprop("[ro.build.version.release]: [14]\n");
        let info = build_info(&map, None, None);
        assert_eq!(info.android_version.as_deref(), Some("14"));
        assert_eq!(info.manufacturer, None);
        assert_eq!(info.system_summary(), "Android 14");
    }

    #[test]
    fn empty_info_summary() {
        assert_eq!(DeviceInfo::default().system_summary(), "Unknown");
    }
}
