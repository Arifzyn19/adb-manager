//! APK action orchestration. Subprocess work goes through [`AdbClient`];
//! callers run these on worker threads and report via events.

use super::inspector::inspect_apk_file;
use super::inspector::ApkInfo;
use crate::adb::{AdbClient, AdbError};
use std::path::Path;

/// Inspect one APK file from disk (ZIP + manifest + signatures).
pub fn inspect_apk(client_display_path: &Path) -> Result<ApkInfo, AdbError> {
    inspect_apk_file(client_display_path)
}

/// Install APK file(s) on a device.
///
/// - validates extensions (`.apk` only — split bundles arrive as several
///   `.apk` files, e.g. `base.apk` + `split_config.*.apk`)
/// - single file → `adb install [-r]`; several → `install-multiple [-r]`
/// - `display` names the app in humanized errors (package when the inspector
///   knows it, else the file name).
pub fn install_apks(
    client: &AdbClient,
    serial: &str,
    files: &[String],
    reinstall: bool,
    display: &str,
) -> Result<String, AdbError> {
    validate_apk_paths(files)?;
    client.install(serial, files, reinstall, display)
}

/// Reject empty selections, missing files and non-APK extensions before any
/// subprocess spawns. Returns the files unchanged on success.
pub fn validate_apk_paths(files: &[String]) -> Result<(), AdbError> {
    if files.is_empty() {
        return Err(AdbError::InvalidApk {
            message: "No APK files selected.".to_string(),
        });
    }
    for f in files {
        if !f.to_ascii_lowercase().ends_with(".apk") {
            return Err(AdbError::InvalidApk {
                message: format!("{f} is not an .apk file."),
            });
        }
        if !Path::new(f).is_file() {
            return Err(AdbError::InvalidApk {
                message: format!("File not found: {f}"),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_non_apk() {
        assert!(validate_apk_paths(&[]).is_err());
        assert!(validate_apk_paths(&["app.zip".to_string()]).is_err());
    }

    #[test]
    fn rejects_missing_files() {
        assert!(validate_apk_paths(&["/definitely/not/here/app.apk".to_string()]).is_err());
    }
}
