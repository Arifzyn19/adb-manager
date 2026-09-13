//! File-browser orchestration. All subprocess work goes through [`AdbClient`];
//! callers run these on worker threads and report via events.
//!
//! Safety notes:
//! - remote paths are validated (`normalize_dir` / `check_mutable_path` /
//!   `check_child_name`) before any `rm`/`mv`/`mkdir` runs
//! - `ls` output is parsed defensively; "empty dir" vs "missing dir" is told
//!   apart via the process exit code + stderr, not row count alone

use super::entry::{check_child_name, check_mutable_path, join_remote, normalize_dir, FileEntry};
use super::parser::parse_ls_long;
use crate::adb::{AdbClient, AdbError};

/// List a remote directory. Returns `(canonical_dir, entries)`.
pub fn list_dir(
    client: &AdbClient,
    serial: &str,
    dir: &str,
) -> Result<(String, Vec<FileEntry>), AdbError> {
    if crate::device::discovery::is_mock() {
        let dir = normalize_dir(dir).unwrap_or_else(|| "/sdcard".to_string());
        return Ok((dir.clone(), mock_entries(&dir)));
    }
    let Some(canonical) = normalize_dir(dir) else {
        return Err(AdbError::ExecutionFailed {
            message: format!("{dir} is not a valid remote directory."),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        });
    };
    let out = client.ls_raw(serial, &canonical)?;
    let stdout = out.stdout_str();
    let stderr = out.stderr_str();
    if !out.success() {
        return Err(map_ls_error(serial, &canonical, &stdout, &stderr));
    }
    let entries = parse_ls_long(&stdout, &canonical);
    if entries.is_empty() && is_missing_dir(&stdout, &stderr) {
        return Err(map_ls_error(serial, &canonical, &stdout, &stderr));
    }
    Ok((canonical, entries))
}

/// Create one folder inside `parent`.
pub fn make_dir(
    client: &AdbClient,
    serial: &str,
    parent: &str,
    name: &str,
) -> Result<String, AdbError> {
    check_child_name(name).map_err(|message| AdbError::ExecutionFailed {
        message,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
    })?;
    let Some(dir) = normalize_dir(parent) else {
        return Err(AdbError::ExecutionFailed {
            message: format!("{parent} is not a valid remote directory."),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        });
    };
    let target = join_remote(&dir, name.trim());
    check_mutable(&target)?;
    client.mkdir(serial, &target)
}

/// Delete a file, link or directory tree (with the root/empty gate).
pub fn delete_path(client: &AdbClient, serial: &str, path: &str) -> Result<String, AdbError> {
    check_mutable(path)?;
    client.remove(serial, path)
}

/// Rename / move `from` → `to` (both absolute remote paths).
pub fn rename_path(
    client: &AdbClient,
    serial: &str,
    from: &str,
    to: &str,
) -> Result<String, AdbError> {
    check_mutable(from)?;
    check_mutable(to)?;
    client.rename(serial, from, to)
}

/// Upload local files into a remote directory. Returns per-file outcomes
/// (`Ok` names); the first hard failure aborts with its error so partial
/// uploads are visible, not silent.
pub fn upload_files(
    client: &AdbClient,
    serial: &str,
    local_files: &[String],
    remote_dir: &str,
) -> Result<Vec<String>, AdbError> {
    if local_files.is_empty() {
        return Err(AdbError::FileTransferFailed {
            message: "No files selected for upload.".to_string(),
        });
    }
    let Some(dir) = normalize_dir(remote_dir) else {
        return Err(AdbError::FileTransferFailed {
            message: format!("{remote_dir} is not a valid remote directory."),
        });
    };
    let mut done = Vec::new();
    for local in local_files {
        // Files and folders alike (`adb push` recurses into directories).
        if !std::path::Path::new(local).exists() {
            return Err(AdbError::FileTransferFailed {
                message: format!("Local path not found: {local}"),
            });
        }
        let file_name = std::path::Path::new(local)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| local.clone());
        let remote = join_remote(&dir, &file_name);
        client.push(serial, local, &remote)?;
        done.push(remote);
    }
    Ok(done)
}

/// Download one remote entry (file or folder — `adb pull` handles both)
/// into a local directory chosen by the user.
pub fn download_entry(
    client: &AdbClient,
    serial: &str,
    remote: &str,
    local_dir: &str,
) -> Result<String, AdbError> {
    let Some(_) = normalize_dir(
        remote
            .rfind('/')
            .map(|i| if i == 0 { "/" } else { &remote[..i] })
            .unwrap_or("/"),
    ) else {
        return Err(AdbError::FileTransferFailed {
            message: format!("{remote} is not a valid remote path."),
        });
    };
    if !std::path::Path::new(local_dir).is_dir() {
        return Err(AdbError::FileTransferFailed {
            message: format!("Local folder not found: {local_dir}"),
        });
    }
    client.pull(serial, remote, local_dir)
}

/// Refuse destructive operations against empty/root inputs, mapped to a
/// typed ADB error.
fn check_mutable(path: &str) -> Result<(), AdbError> {
    check_mutable_path(path).map_err(|message| AdbError::ExecutionFailed {
        message,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
    })
}

fn is_missing_dir(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{stdout}\n{stderr}").to_lowercase();
    combined.contains("no such file or directory")
}

fn map_ls_error(serial: &str, dir: &str, stdout: &str, stderr: &str) -> AdbError {
    let combined = format!("{stdout}\n{stderr}").to_lowercase();
    if combined.contains("permission denied") || combined.contains("operation not permitted") {
        AdbError::PermissionDenied {
            message: format!("Cannot read {dir}: the device denied access."),
        }
    } else if combined.contains("unauthorized") {
        AdbError::DeviceUnauthorized {
            serial: serial.to_string(),
        }
    } else if combined.contains("no such file or directory") || combined.contains("not a directory")
    {
        AdbError::ExecutionFailed {
            message: format!("{dir} does not exist on this device (or is not a directory)."),
            exit_code: None,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    } else {
        AdbError::ExecutionFailed {
            message: format!("Could not list {dir}: {}", stderr.trim()),
            exit_code: None,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }
}

fn mock_entries(dir: &str) -> Vec<FileEntry> {
    use super::entry::FileKind;
    vec![
        FileEntry {
            name: "DCIM".to_string(),
            path: join_remote(dir, "DCIM"),
            kind: FileKind::Dir,
            size_bytes: 4096,
            perms: "drwxr-xr-x".to_string(),
            owner: "root".to_string(),
            group: "sdcard_rw".to_string(),
            modified: "2024-05-01 12:00".to_string(),
            link_target: None,
        },
        FileEntry {
            name: "Download".to_string(),
            path: join_remote(dir, "Download"),
            kind: FileKind::Dir,
            size_bytes: 4096,
            perms: "drwxr-xr-x".to_string(),
            owner: "root".to_string(),
            group: "sdcard_rw".to_string(),
            modified: "2024-05-01 12:00".to_string(),
            link_target: None,
        },
        FileEntry {
            name: "notes.txt".to_string(),
            path: join_remote(dir, "notes.txt"),
            kind: FileKind::File,
            size_bytes: 2048,
            perms: "-rw-rw----".to_string(),
            owner: "root".to_string(),
            group: "sdcard_rw".to_string(),
            modified: "2024-05-01 12:01".to_string(),
            link_target: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_dir_rejects_bad_names() {
        let client = AdbClient::new(std::path::PathBuf::from("adb"));
        assert!(make_dir(&client, "S", "/sdcard", "../evil").is_err());
        assert!(make_dir(&client, "S", "/sdcard", "").is_err());
        assert!(make_dir(&client, "S", "/sdcard", "a/b").is_err());
    }

    #[test]
    fn delete_refuses_root_and_empty() {
        let client = AdbClient::new(std::path::PathBuf::from("adb"));
        assert!(delete_path(&client, "S", "/").is_err());
        assert!(delete_path(&client, "S", "").is_err());
    }

    #[test]
    fn rename_guards_both_ends() {
        let client = AdbClient::new(std::path::PathBuf::from("adb"));
        assert!(rename_path(&client, "S", "/", "/sdcard/x").is_err());
        assert!(rename_path(&client, "S", "/sdcard/x", "/").is_err());
    }

    #[test]
    fn upload_rejects_empty_and_missing() {
        let client = AdbClient::new(std::path::PathBuf::from("adb"));
        assert!(upload_files(&client, "S", &[], "/sdcard").is_err());
        assert!(upload_files(&client, "S", &["/nope/missing.bin".to_string()], "/sdcard").is_err());
    }
}
