//! Parsers for `pm list packages`, `dumpsys package`, `pm path` and `ps`.
//!
//! All parsers are best-effort: dumpsys layouts drift across Android
//! versions and OEM skins, so unknown lines are skipped, never fatal.

use super::package::{AppInfo, PermissionStatus};
use std::collections::HashSet;

/// Parse `pm list packages` output.
///
/// Accepts both plain (`package:com.x`) and `-f` form
/// (`package:/data/app/…/base.apk=com.x`).
pub fn parse_pm_list(output: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("package:") else {
            continue;
        };
        // `-f` form keeps the package after the last `=`.
        let pkg = rest.rsplit('=').next().unwrap_or(rest).trim();
        if !pkg.is_empty() {
            out.push(pkg.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Parse `pm path PKG` output into on-device APK paths.
pub fn parse_pm_path(output: &str) -> Vec<String> {
    parse_pm_list(output)
        .into_iter()
        .filter(|p| p.starts_with('/'))
        .collect()
}

/// Parse `adb shell ps -A` output into process names (last column).
/// Returns an empty set when the format is unrecognized.
pub fn parse_ps_names(output: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Skip the header row.
        if line.starts_with("USER") || line.contains("PID") && line.contains("NAME") {
            continue;
        }
        if let Some(name) = line.split_whitespace().last() {
            if !name.is_empty() && name != "NAME" {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// Parse `dumpsys package PKG` into [`AppInfo`].
pub fn parse_dumpsys_package(output: &str, package: &str, system: bool) -> AppInfo {
    let mut info = AppInfo {
        package: package.to_string(),
        enabled: true,
        system,
        ..Default::default()
    };
    let mut saw_package_block = false;
    let mut section = Section::None;
    // Which permission block a `name: granted=…` line belongs to.
    let mut runtime_block = false;

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        // Resolver-table section headers (column-0 or `X Resolver Table:`).
        if let Some(next) = section_header(line) {
            section = next;
            continue;
        }
        // `Packages:` / `Package [x] (...):` resets to package metadata.
        if line == "Packages:" || line.starts_with("Package [") {
            section = Section::Package;
            if line.starts_with("Package [") {
                saw_package_block = true;
            }
            continue;
        }

        match section {
            Section::Package => {
                if line == "install permissions:" {
                    runtime_block = false;
                    continue;
                }
                if line == "runtime permissions:" {
                    runtime_block = true;
                    continue;
                }
                parse_package_line(&mut info, line, &mut saw_package_block, runtime_block);
            }
            Section::Activities | Section::Services | Section::Receivers | Section::Providers => {
                if let Some(component) = component_token(line) {
                    let list = match section {
                        Section::Activities => &mut info.activities,
                        Section::Services => &mut info.services,
                        Section::Receivers => &mut info.receivers,
                        _ => &mut info.providers,
                    };
                    if !list.contains(&component) {
                        list.push(component);
                    }
                }
            }
            Section::None => {}
        }
    }

    for list in [
        &mut info.activities,
        &mut info.services,
        &mut info.receivers,
        &mut info.providers,
    ] {
        list.sort();
    }
    info.partial = !saw_package_block && info.version_name.is_none();
    info
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Package,
    Activities,
    Services,
    Receivers,
    Providers,
}

fn section_header(line: &str) -> Option<Section> {
    match line {
        "Activity Resolver Table:" => Some(Section::Activities),
        "Service Resolver Table:" => Some(Section::Services),
        "Receiver Resolver Table:" => Some(Section::Receivers),
        "Provider Resolver Table:" => Some(Section::Providers),
        _ => None,
    }
}

fn parse_package_line(info: &mut AppInfo, line: &str, saw_block: &mut bool, runtime_block: bool) {
    // `versionCode=300101 minSdk=24 targetSdk=34`
    if let Some(rest) = line.strip_prefix("versionCode=") {
        *saw_block = true;
        info.version_code = rest.split_whitespace().next().map(str::to_string);
        return;
    }
    if let Some(rest) = line.strip_prefix("versionName=") {
        *saw_block = true;
        info.version_name = Some(rest.trim().to_string());
        return;
    }
    // `application-label:'TikTok'`
    if let Some(rest) = line.strip_prefix("application-label:") {
        *saw_block = true;
        info.label = Some(rest.trim().trim_matches('\'').to_string());
        return;
    }
    if let Some(rest) = line.strip_prefix("installerPackageName=") {
        info.installer = Some(rest.trim().to_string());
        return;
    }
    if let Some(rest) = line.strip_prefix("userId=") {
        info.uid = rest.split_whitespace().next().map(str::to_string);
        return;
    }
    if let Some(rest) = line.strip_prefix("enabled=") {
        let v = rest.split_whitespace().next().unwrap_or("").to_lowercase();
        info.enabled = v == "true" || v == "1" || v == "default";
        return;
    }
    // Permission blocks.
    if line == "install permissions:" || line == "runtime permissions:" {
        return;
    }
    if let Some((name, granted)) = parse_permission_line(line) {
        let entry = PermissionStatus { name, granted };
        if runtime_block {
            info.runtime_permissions.push(entry);
        } else {
            info.install_permissions.push(entry);
        }
    }
}

/// Parse `name: granted=true[, flags=…]` permission lines.
fn parse_permission_line(line: &str) -> Option<(String, bool)> {
    let (name, rest) = line.split_once(':')?;
    let name = name.trim();
    if name.is_empty() || name.contains(' ') || !name.contains('.') {
        return None;
    }
    let granted = rest.to_lowercase().contains("granted=true");
    Some((name.to_string(), granted))
}

/// Extract a component name token (`com.x/.Main`) from a resolver line like
/// `5f6a7b8 com.tiktok/.MainActivity filter 12ab34`.
fn component_token(line: &str) -> Option<String> {
    for token in line.split_whitespace() {
        let token = token.trim_end_matches(':');
        if token.contains('/') {
            let (pkg, cls) = token.split_once('/')?;
            if cls.is_empty() || cls.contains(' ') {
                continue;
            }
            // Package part is dotted or empty (shorthand `.Main` handled by caller context).
            if pkg.is_empty() || pkg.contains('.') {
                return Some(token.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const PM_LIST: &str = "package:com.android.chrome\n\
        package:com.example.tiktok\n\
        package:/data/app/com.example.tiktok-1/base.apk=com.example.tiktok\n\
        not-a-package-line\n";

    #[test]
    fn parses_pm_list_plain_and_pathed() {
        assert_eq!(
            parse_pm_list(PM_LIST),
            vec!["com.android.chrome", "com.example.tiktok"]
        );
    }

    #[test]
    fn parses_pm_path() {
        let out = "package:/data/app/com.x-1/base.apk\n\
            package:/data/app/com.x-1/split_config.arm64_v8a.apk\n";
        let paths = parse_pm_path(out);
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("base.apk"));
    }

    #[test]
    fn parses_ps_names() {
        let out = "USER           PID  PPID     VSZ    RSS WCHAN            ADDR S NAME\n\
            u0_a152      12345    678 123456  78900                0 S com.example.tiktok\n\
            root             1      0  10000   2000                0 S init\n";
        let names = parse_ps_names(out);
        assert!(names.contains("com.example.tiktok"));
        assert!(names.contains("init"));
        assert!(!names.contains("USER"));
    }

    const DUMPSYS_SAMPLE: &str = "\
Activity Resolver Table:\n\
  Schemes:\n\
      https:\n\
        12ab34 com.example.tiktok/.MainActivity filter 56cd78\n\
        12ab35 com.example.tiktok/.ShareActivity filter 56cd79\n\
Service Resolver Table:\n\
  Non-Data Actions:\n\
      android.intent.action.SYNC:\n\
        99ff00 com.example.tiktok/.SyncService filter 11aa22\n\
Packages:\n\
  Package [com.example.tiktok] (a1b2c3):\n\
    userId=10152\n\
    versionCode=300101 minSdk=24 targetSdk=34\n\
    versionName=30.1.1\n\
    application-label:'TikTok'\n\
    installerPackageName=com.android.vending\n\
    enabled=true\n\
    install permissions:\n\
      android.permission.INTERNET: granted=true\n\
      com.google.android.c2dm.permission.RECEIVE: granted=true\n\
";

    #[test]
    fn parses_dumpsys_version_block() {
        let info = parse_dumpsys_package(DUMPSYS_SAMPLE, "com.example.tiktok", false);
        assert_eq!(info.version_name.as_deref(), Some("30.1.1"));
        assert_eq!(info.version_code.as_deref(), Some("300101"));
        assert_eq!(info.label.as_deref(), Some("TikTok"));
        assert_eq!(info.uid.as_deref(), Some("10152"));
        assert_eq!(info.installer.as_deref(), Some("com.android.vending"));
        assert!(info.enabled);
        assert!(!info.partial);
        assert_eq!(info.install_permissions.len(), 2);
        assert!(info.install_permissions[0].granted);
    }

    #[test]
    fn parses_resolver_components() {
        let info = parse_dumpsys_package(DUMPSYS_SAMPLE, "com.example.tiktok", false);
        assert_eq!(
            info.activities,
            vec![
                "com.example.tiktok/.MainActivity",
                "com.example.tiktok/.ShareActivity"
            ]
        );
        assert_eq!(info.services, vec!["com.example.tiktok/.SyncService"]);
        assert!(info.receivers.is_empty());
        assert!(info.providers.is_empty());
    }

    #[test]
    fn unknown_package_yields_partial_info() {
        let info = parse_dumpsys_package("Can't find package: com.ghost\n", "com.ghost", false);
        assert!(info.partial);
        assert_eq!(info.package, "com.ghost");
    }

    #[test]
    fn permission_line_parsing() {
        assert_eq!(
            parse_permission_line("android.permission.CAMERA: granted=true"),
            Some(("android.permission.CAMERA".to_string(), true))
        );
        assert_eq!(
            parse_permission_line("android.permission.LOCATION: granted=false, flags=[ X]"),
            Some(("android.permission.LOCATION".to_string(), false))
        );
        assert_eq!(parse_permission_line("  12ab34 com.x/.A filter 1"), None);
        assert_eq!(parse_permission_line("versionName=1.0"), None);
    }

    #[test]
    fn runtime_permissions_go_to_runtime_list() {
        let out = "Packages:\n\
            Package [com.x] (1):\n\
            versionName=1.0\n\
            install permissions:\n\
              android.permission.INTERNET: granted=true\n\
            runtime permissions:\n\
              android.permission.CAMERA: granted=false\n";
        let info = parse_dumpsys_package(out, "com.x", false);
        assert_eq!(info.install_permissions.len(), 1);
        assert_eq!(info.runtime_permissions.len(), 1);
        assert!(!info.runtime_permissions[0].granted);
    }
}
