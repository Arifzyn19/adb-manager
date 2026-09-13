//! App fetch + action orchestration. All subprocess work goes through
//! [`AdbClient`]; callers run these on worker threads and report via events.

use super::package::{AppInfo, PackageEntry};
use super::parser;
use crate::adb::{AdbClient, AdbError, PackageFilter};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Fast pass: full package list + system membership + running process names.
pub fn fetch_package_entries(
    client: &AdbClient,
    serial: &str,
) -> Result<(Vec<PackageEntry>, HashSet<String>), AdbError> {
    if crate::device::discovery::is_mock() {
        return Ok((mock_entries(), mock_running()));
    }
    let all = client.shell_text(serial, &["pm", "list", "packages"])?;
    let all = parser::parse_pm_list(&all);
    // System membership via a second listing; failure degrades to "all user".
    let system: HashSet<String> = client
        .run(
            &client
                .builder()
                .pm_list_packages(serial, PackageFilter::System),
        )?
        .stdout_str()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .collect();
    let entries = all
        .into_iter()
        .map(|package| PackageEntry {
            system: system.contains(&package),
            package,
        })
        .collect();
    let running = fetch_running_set(client, serial);
    Ok((entries, running))
}

/// Best-effort running-process names; empty set when `ps` is unavailable.
pub fn fetch_running_set(client: &AdbClient, serial: &str) -> HashSet<String> {
    client
        .run(&client.builder().ps_all(serial))
        .ok()
        .filter(|o| o.success())
        .map(|o| parser::parse_ps_names(&o.stdout_str()))
        .unwrap_or_default()
}

/// Full details for one package (`dumpsys package` + `pm path`).
pub fn fetch_app_details(
    client: &AdbClient,
    serial: &str,
    package: &str,
    system: bool,
    running: bool,
) -> Result<AppInfo, AdbError> {
    if crate::device::discovery::is_mock() {
        return Ok(mock_details(package));
    }
    let dumpsys = client.shell_text(serial, &["dumpsys", "package", package])?;
    let mut info = parser::parse_dumpsys_package(&dumpsys, package, system);
    info.running = running;
    if let Ok(paths) = client.shell_text(serial, &["pm", "path", package]) {
        info.apk_paths = parser::parse_pm_path(&paths);
    }
    Ok(info)
}

/// `monkey` launch. Returns raw output on success.
pub fn launch_app(client: &AdbClient, serial: &str, package: &str) -> Result<String, AdbError> {
    let out = client.run(&client.builder().launch_app(serial, package))?;
    let text = format!("{}{}", out.stdout_str(), out.stderr_str());
    if out.success() && !text.to_lowercase().contains("no activities") {
        Ok(text.trim().to_string())
    } else {
        Err(AdbError::ExecutionFailed {
            message: format!("No launchable activity found for {package}"),
            exit_code: out.exit_code,
            stdout: out.stdout_str(),
            stderr: out.stderr_str(),
        })
    }
}

pub fn force_stop_app(client: &AdbClient, serial: &str, package: &str) -> Result<String, AdbError> {
    let out = client.run(&client.builder().force_stop(serial, package))?;
    if out.success() {
        Ok(format!("Force-stopped {package}"))
    } else {
        Err(AdbError::ExecutionFailed {
            message: format!("force-stopping {package} failed"),
            exit_code: out.exit_code,
            stdout: out.stdout_str(),
            stderr: out.stderr_str(),
        })
    }
}

/// `pm clear [--cache-only]`. `cache_only` needs a newer Android; failure
/// surfaces as "not available on this device" via the caller.
pub fn clear_app(
    client: &AdbClient,
    serial: &str,
    package: &str,
    cache_only: bool,
) -> Result<String, AdbError> {
    let out = client.run(&client.builder().pm_clear(serial, package, cache_only))?;
    let text = format!("{}{}", out.stdout_str(), out.stderr_str());
    if out.success() && text.to_lowercase().contains("success") {
        Ok(text.trim().to_string())
    } else {
        Err(AdbError::ExecutionFailed {
            message: if cache_only {
                "Clear-cache is not available on this device.".to_string()
            } else {
                format!("Clearing data for {package} failed: {}", text.trim())
            },
            exit_code: out.exit_code,
            stdout: out.stdout_str(),
            stderr: out.stderr_str(),
        })
    }
}

pub fn uninstall_app(client: &AdbClient, serial: &str, package: &str) -> Result<String, AdbError> {
    client.uninstall(serial, package)
}

/// On-device APK paths for a package.
pub fn apk_paths(client: &AdbClient, serial: &str, package: &str) -> Result<Vec<String>, AdbError> {
    Ok(parser::parse_pm_path(
        &client.shell_text(serial, &["pm", "path", package])?,
    ))
}

/// Pull every APK split into `dest_dir`, returning the local files.
/// Filenames are derived from the remote basename; splits keep theirs
/// (`base.apk`, `split_config.arm64_v8a.apk`, …).
pub fn pull_apk(
    client: &AdbClient,
    serial: &str,
    package: &str,
    dest_dir: &Path,
) -> Result<Vec<PathBuf>, AdbError> {
    let remotes = apk_paths(client, serial, package)?;
    if remotes.is_empty() {
        return Err(AdbError::ExecutionFailed {
            message: format!("No APK path reported for {package}"),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        });
    }
    std::fs::create_dir_all(dest_dir).map_err(AdbError::Io)?;
    let mut pulled = Vec::new();
    for remote in &remotes {
        let file_name = remote
            .rsplit('/')
            .next()
            .filter(|n| n.ends_with(".apk"))
            .unwrap_or("base.apk");
        // Prefix with the package to avoid collisions across extractions.
        let local = dest_dir.join(format!("{package}_{file_name}"));
        client.pull(serial, remote, &local.to_string_lossy())?;
        pulled.push(local);
    }
    Ok(pulled)
}

// --- Mock data (ADB_MANAGER_MOCK=1) ----------------------------------------

fn mock_entries() -> Vec<PackageEntry> {
    vec![
        PackageEntry {
            package: "com.example.tiktok".to_string(),
            system: false,
        },
        PackageEntry {
            package: "com.android.chrome".to_string(),
            system: false,
        },
        PackageEntry {
            package: "com.android.settings".to_string(),
            system: true,
        },
        PackageEntry {
            package: "android".to_string(),
            system: true,
        },
    ]
}

fn mock_running() -> HashSet<String> {
    ["com.example.tiktok".to_string()].into_iter().collect()
}

fn mock_details(package: &str) -> AppInfo {
    AppInfo {
        package: package.to_string(),
        label: Some(package.rsplit('.').next().unwrap_or(package).to_string()),
        version_name: Some("1.2.3".to_string()),
        version_code: Some("10203".to_string()),
        uid: Some("10152".to_string()),
        installer: Some("com.android.vending".to_string()),
        enabled: true,
        system: package.starts_with("com.android") || package == "android",
        running: mock_running().contains(package),
        apk_paths: vec![format!("/data/app/{package}/base.apk")],
        install_permissions: vec![],
        runtime_permissions: vec![],
        activities: vec![format!("{package}/.MainActivity")],
        services: vec![],
        receivers: vec![],
        providers: vec![],
        partial: false,
    }
}
