//! Robust parser for `adb devices -l` (and `adb devices`) output.
//!
//! Parsing is defensive: unknown columns / states are preserved rather than
//! causing failures, because ADB output varies across platform-tools versions,
//! manufacturers and transports (USB, wireless, emulator).

use super::device::{Device, DeviceState, Transport};
use std::collections::HashMap;

/// Parse full stdout of `adb devices -l` into a device list.
///
/// Tolerates:
/// - missing header line
/// - blank lines, `* daemon started` noise lines
/// - `unauthorized` / `offline` lines without `-l` details
/// - unknown states (kept as `DeviceState::Unknown` + raw token)
pub fn parse_devices_long(output: &str) -> Vec<Device> {
    let mut devices = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Header + daemon noise are not devices.
        if line.starts_with("List of devices") {
            continue;
        }
        if line.starts_with('*') || line.starts_with("adb:") {
            continue;
        }
        if let Some(device) = parse_device_line(line) {
            devices.push(device);
        }
    }

    devices
}

fn parse_device_line(line: &str) -> Option<Device> {
    let mut parts = line.split_whitespace();
    let serial = parts.next()?.to_string();
    // Guard: a stray header remnant or garbage line has no state token.
    let raw_state = parts.next()?.to_string();

    let mut details: HashMap<&str, &str> = HashMap::new();
    for token in parts {
        if let Some((k, v)) = token.split_once(':') {
            // Only treat well-formed key:value detail tokens as details.
            if !k.is_empty() {
                details.insert(k, v);
            }
        }
    }

    let state = DeviceState::from_adb_token(&raw_state);
    let model = details.get("model").map(|s| s.to_string());
    let product = details.get("product").map(|s| s.to_string());
    let device_name = details.get("device").map(|s| s.to_string());
    let transport_id = details.get("transport_id").map(|s| s.to_string());
    let usb = details.get("usb").map(|s| s.to_string());

    let mut transport = Transport::infer_from_serial(&serial);
    // `adb devices -l` prints `usb:...` for real USB devices; a serial that
    // looks like an IP but carries usb info is still USB. Wireless devices
    // never carry a `usb:` detail.
    if usb.is_some() {
        transport = Transport::Usb;
    }

    Some(Device {
        serial,
        state,
        raw_state,
        transport,
        model,
        product,
        device_name,
        transport_id,
        usb,
    })
}

/// Parse `adb version` stdout, e.g. "Android Debug Bridge version 1.0.41".
pub fn parse_adb_version(output: &str) -> Option<String> {
    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Android Debug Bridge version ") {
            let ver = rest.split_whitespace().next().unwrap_or("").trim();
            if !ver.is_empty() {
                return Some(ver.to_string());
            }
        }
    }
    None
}

/// True when `adb connect` output reports an actual connection.
/// ADB exits 0 even for some failures, so the text must be inspected:
/// "connected to 1.2.3.4:5555" / "already connected to …" count as success.
pub fn is_connect_success(output: &str) -> bool {
    let lower = output.to_lowercase();
    lower.contains("connected to") || lower.contains("already connected")
}

/// True when `adb pair` output reports "Successfully paired to …".
pub fn is_pair_success(output: &str) -> bool {
    output.to_lowercase().contains("successfully paired")
}

/// Classify a pairing/connect error message into a human-readable hint.
/// Kept here (not in UI) so behavior is unit-tested.
pub fn humanize_adb_error(stderr: &str) -> String {
    let lower = stderr.to_lowercase();
    if lower.contains("failed to authenticate") || lower.contains("wrong pairing code") {
        "Incorrect pairing code. Double-check the code on the phone and retry.".to_string()
    } else if lower.contains("failed to connect") && lower.contains("refused") {
        "Connection refused. Use the ADB connection port from Wireless Debugging (not the pairing port)."
            .to_string()
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "Timed out. Make sure the phone and PC are on the same Wi-Fi network.".to_string()
    } else if lower.contains("unauthorized") {
        "Device is unauthorized. Accept the debugging prompt on the phone.".to_string()
    } else if lower.contains("no such host") || lower.contains("could not resolve") {
        "Could not resolve the IP address. Check it for typos.".to_string()
    } else {
        stderr.trim().to_string()
    }
}

/// True when `adb install` output reports success.
///
/// ADB prints `Success` on stdout on success and `Failure [CODE]` or
/// `INSTALL_FAILED_*` on failure. Case-insensitive on purpose: OEM builds
/// vary the casing.
pub fn is_install_success(output: &str) -> bool {
    let lower = output.to_lowercase();
    // "Failure [INSTALL_FAILED_...]" contains neither bare "success" hit by
    // accident: require the success token without a failure marker.
    lower.contains("success") && !lower.contains("failure") && !lower.contains("failed")
}

/// Human-readable explanation for file operations (`ls`, `mkdir`, `rm`,
/// `mv`, `push`, `pull`) while preserving the original output.
pub fn humanize_file_error(output: &str, op: &str, target: &str) -> String {
    let lower = output.to_lowercase();
    let hint = if lower.contains("permission denied") || lower.contains("operation not permitted") {
        "The device refused this operation (permission denied). System and app-private directories are off-limits without root."
    } else if lower.contains("read-only file system") {
        "The target filesystem is read-only on this device."
    } else if lower.contains("no such file or directory") {
        "The path does not exist (it may have been removed)."
    } else if lower.contains("directory not empty") {
        "The directory is not empty."
    } else if lower.contains("is a directory") {
        "That path is a directory — pick a file, or download the whole folder."
    } else if lower.contains("not a directory") {
        "A component of that path is not a directory."
    } else if lower.contains("file exists") {
        "A file with that name already exists."
    } else if lower.contains("no space") || lower.contains("enospc") {
        "No space left on the device."
    } else {
        return format!("{op} {target} failed: {}", output.trim());
    };
    format!("{hint} [{op} {target}: {}]", output.trim())
}

/// Human-readable explanation for install/uninstall failures while
/// preserving the original ADB error text.
pub fn humanize_install_error(output: &str, package: &str) -> String {
    let lower = output.to_lowercase();
    let hint = if lower.contains("install_failed_already_exists") {
        "The app is already installed. Retry with Reinstall (-r) enabled."
    } else if lower.contains("install_failed_invalid_apk") || lower.contains("install_parse_failed")
    {
        "The APK file is invalid or corrupted. Re-download / rebuild it."
    } else if lower.contains("install_failed_test_only") || lower.contains("test-only") {
        "This is a test-only APK. It cannot be installed with plain `adb install`."
    } else if lower.contains("install_failed_insufficient_storage")
        || lower.contains("install_failed_container_error")
    {
        "Not enough storage on the device. Free space and retry."
    } else if lower.contains("install_failed_update_incompatible")
        || lower.contains("install_failed_conflicting_provider")
        || lower.contains("install_failed_duplicate_permission")
    {
        "A conflicting package is installed. Uninstall it first, then retry."
    } else if lower.contains("install_failed_version_downgrade")
        || lower.contains("install_failed_permission_model_downgrade")
    {
        "The existing app is newer. Uninstall it first to downgrade."
    } else if lower.contains("install_failed_inconsistent_certificates")
        || lower.contains("signatures do not match")
        || lower.contains("signature mismatch")
    {
        "Signature conflict: the installed app was signed with a different key. Uninstall it first (data will be lost)."
    } else if lower.contains("install_failed_missing_shared_library")
        || lower.contains("install_failed_missing_feature")
    {
        "The app needs a shared library / feature missing on this device."
    } else if lower.contains("install_failed_older_sdk")
        || lower.contains("install_failed_newer_sdk")
    {
        "Incompatible SDK: the app requires a different Android version."
    } else if lower.contains("install_failed_user_restricted") {
        "Installation is blocked by the device owner / user restrictions."
    } else if lower.contains("device unauthorized") || lower.contains("unauthorized") {
        "Device is unauthorized. Accept the debugging prompt on the phone."
    } else if lower.contains("no devices") || lower.contains("device not found") {
        "The device disconnected during installation. Reconnect and retry."
    } else if lower.contains("delete_failed_device_policy_manager") {
        "A device-policy manager blocks uninstalling this app."
    } else if lower.contains("delete_failed_owner_blocked") {
        "The profile owner blocks uninstalling this app."
    } else if lower.contains("not installed for") || lower.contains("unknown package") {
        "The package is not installed on this device."
    } else if lower.contains("system package") || lower.contains("system app") {
        "System apps cannot be uninstalled this way (disable them in Settings instead)."
    } else {
        return format!("Operation on {package} failed: {}", output.trim());
    };
    format!("{hint} [{}]", output.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "List of devices attached\n\
        emulator-5554          device product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64 device:emulator64_x86_64 transport_id:1\n\
        0A201JEC200123         device product:I2219 model:vivo_I2219 device:I2219 transport_id:2\n\
        ABCD1234               unauthorized transport_id:3\n\
        192.168.1.20:39001     device product:oriole model:Pixel_6 device:oriole transport_id:4\n\
        XYZ999                 offline transport_id:5\n\
        \n";

    #[test]
    fn parses_normal_usb_device() {
        let devices = parse_devices_long(SAMPLE);
        let vivo = devices
            .iter()
            .find(|d| d.serial == "0A201JEC200123")
            .unwrap();
        assert_eq!(vivo.state, DeviceState::Connected);
        assert_eq!(vivo.transport, Transport::Usb);
        assert_eq!(vivo.model.as_deref(), Some("vivo_I2219"));
        assert_eq!(vivo.display_name(), "vivo I2219");
    }

    #[test]
    fn parses_emulator_wireless_unauthorized_offline() {
        let devices = parse_devices_long(SAMPLE);
        assert_eq!(devices.len(), 5);

        let emu = &devices[0];
        assert_eq!(emu.transport, Transport::Emulator);
        assert_eq!(emu.state, DeviceState::Connected);

        let wl = devices
            .iter()
            .find(|d| d.serial == "192.168.1.20:39001")
            .unwrap();
        assert_eq!(wl.transport, Transport::Wireless);
        assert_eq!(wl.model.as_deref(), Some("Pixel_6"));

        let unauth = devices.iter().find(|d| d.serial == "ABCD1234").unwrap();
        assert_eq!(unauth.state, DeviceState::Unauthorized);
        assert_eq!(unauth.model, None);

        let offline = devices.iter().find(|d| d.serial == "XYZ999").unwrap();
        assert_eq!(offline.state, DeviceState::Offline);
    }

    #[test]
    fn ignores_noise_and_headerless_output() {
        let out = "* daemon not running; starting now\nABCD device product:x model:Y device:Z\n";
        let devices = parse_devices_long(out);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial, "ABCD");
    }

    #[test]
    fn empty_output_gives_no_devices() {
        assert!(parse_devices_long("List of devices attached\n\n").is_empty());
        assert!(parse_devices_long("").is_empty());
    }

    #[test]
    fn unknown_state_is_preserved() {
        let devices = parse_devices_long("SERIAL123\trecovery\n");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].state, DeviceState::Unknown);
        assert_eq!(devices[0].raw_state, "recovery");
    }

    #[test]
    fn parses_adb_version_string() {
        let out = "Android Debug Bridge version 1.0.41\nVersion 34.0.5-10985685\nInstalled as C:\\platform-tools\\adb.exe\n";
        assert_eq!(parse_adb_version(out).as_deref(), Some("1.0.41"));
        assert_eq!(parse_adb_version("garbage"), None);
    }

    #[test]
    fn humanizes_common_errors() {
        assert!(humanize_adb_error("failed to authenticate to 1.2.3.4").contains("pairing code"));
        assert!(humanize_adb_error("Failed to connect: refused").contains("pairing port"));
        assert!(humanize_adb_error("unauthorized device").contains("unauthorized"));
    }

    #[test]
    fn connect_success_detection() {
        assert!(is_connect_success("connected to 192.168.1.10:39001\n"));
        assert!(is_connect_success(
            "already connected to 192.168.1.10:39001\n"
        ));
        assert!(!is_connect_success(
            "failed to connect to 192.168.1.10:39001\n"
        ));
        assert!(!is_connect_success("cannot connect: timeout\n"));
    }

    #[test]
    fn pair_success_detection() {
        assert!(is_pair_success(
            "Successfully paired to 192.168.1.5:37001 [guid=adb-abc]\n"
        ));
        assert!(!is_pair_success("Failed to pair to 192.168.1.5:37001\n"));
        assert!(!is_pair_success(""));
    }

    #[test]
    fn install_errors_are_humanized() {
        let msg = humanize_install_error("Failure [INSTALL_FAILED_INSUFFICIENT_STORAGE]", "com.x");
        assert!(msg.contains("storage"));
        assert!(msg.contains("INSTALL_FAILED_INSUFFICIENT_STORAGE"));
        let msg = humanize_install_error("Failure [DELETE_FAILED_DEVICE_POLICY_MANAGER]", "com.x");
        assert!(msg.contains("policy"));
        let msg = humanize_install_error("Failure [INSTALL_FAILED_ALREADY_EXISTS]", "com.x");
        assert!(msg.contains("-r"));
        let msg = humanize_install_error(
            "Failure [INSTALL_FAILED_INCONSISTENT_CERTIFICATES]",
            "com.x",
        );
        assert!(msg.contains("Signature conflict"));
        let msg = humanize_install_error("Failure [weird new code]", "com.x");
        assert!(msg.contains("com.x"));
        assert!(msg.contains("weird new code"));
    }

    #[test]
    fn install_success_detection() {
        assert!(is_install_success("Success\n"));
        assert!(is_install_success("success\n"));
        assert!(!is_install_success(
            "Failure [INSTALL_FAILED_ALREADY_EXISTS]\n"
        ));
        assert!(!is_install_success(""));
    }

    #[test]
    fn file_errors_are_humanized() {
        let msg = humanize_file_error("rm: /x: Permission denied", "Delete", "/x");
        assert!(msg.contains("Permission denied") || msg.contains("permission denied"));
        assert!(msg.contains("/x"));
        let msg = humanize_file_error("ls: /y: No such file or directory", "Browse", "/y");
        assert!(msg.contains("does not exist"));
        let msg = humanize_file_error("weird output", "Delete", "/z");
        assert!(msg.contains("/z"));
        assert!(msg.contains("weird output"));
    }
}
