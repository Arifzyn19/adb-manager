//! Android file-browser model + remote-path helpers.
//!
//! Remote paths always use `/` separators. Everything is passed to ADB as
//! explicit argv (never through a shell), but paths are still validated so
//! the UI fails fast on empty/relative inputs instead of running `rm` with
//! garbage.

use serde::{Deserialize, Serialize};

/// Entry kind derived from the `ls` permission field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FileKind {
    Dir,
    #[default]
    File,
    Symlink,
    Other,
}

impl FileKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Dir => "Folder",
            Self::File => "File",
            Self::Symlink => "Link",
            Self::Other => "Other",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Dir => "📁",
            Self::File => "📄",
            Self::Symlink => "🔗",
            Self::Other => "❓",
        }
    }

    pub fn from_perms(perms: &str) -> Self {
        match perms.chars().next() {
            Some('d') => Self::Dir,
            Some('l') => Self::Symlink,
            Some('-') => Self::File,
            _ => Self::Other,
        }
    }
}

/// One row of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Bare name (`DCIM`, `photo.jpg`).
    pub name: String,
    /// Full remote path (`/sdcard/DCIM`).
    pub path: String,
    pub kind: FileKind,
    pub size_bytes: u64,
    /// Raw `ls` permission field (`drwxr-xr-x`).
    pub perms: String,
    pub owner: String,
    pub group: String,
    /// Raw date/time remainder (`2024-05-01 12:00`, best-effort display only).
    pub modified: String,
    /// Symlink target (`a -> b` gives `Some("b")`).
    pub link_target: Option<String>,
}

impl FileEntry {
    pub fn size_display(&self) -> String {
        if self.kind == FileKind::Dir {
            return "—".to_string();
        }
        crate::apk::inspector::human_size(self.size_bytes)
    }
}

/// Join a child name onto a remote directory (`/sdcard` + `DCIM`).
pub fn join_remote(dir: &str, name: &str) -> String {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Parent of a remote path (`/sdcard/DCIM` → `/sdcard`; `/` → `/`).
pub fn parent_dir(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/".to_string();
    }
    match trimmed.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => trimmed[..i].to_string(),
    }
}

/// Normalize a remote directory: absolute, no trailing slash (except root),
/// collapse duplicate slashes. Returns `None` for empty/relative inputs.
pub fn normalize_dir(input: &str) -> Option<String> {
    let t = input.trim();
    if t.is_empty() || !t.starts_with('/') || t.contains('\0') {
        return None;
    }
    let mut out = String::new();
    for part in t.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            // Pop one level; never escape the root.
            if let Some(i) = out.rfind('/') {
                out.truncate(i);
            }
            continue;
        }
        out.push('/');
        out.push_str(part);
    }
    if out.is_empty() {
        out.push('/');
    }
    Some(out)
}

/// Refuse destructive operations against empty/root inputs. Browsing is
/// allowed anywhere; only `rm`/`mv`-class calls go through this gate.
pub fn check_mutable_path(path: &str) -> Result<(), String> {
    let Some(norm) = normalize_dir(path) else {
        return Err(format!("Refusing to modify invalid path: {path}"));
    };
    if norm == "/" {
        return Err("Refusing to modify the filesystem root.".to_string());
    }
    Ok(())
}

/// A new child name typed by the user: no slashes, no NUL, not `.`/`..`.
pub fn check_child_name(name: &str) -> Result<(), String> {
    let t = name.trim();
    if t.is_empty() {
        return Err("Name must not be empty.".to_string());
    }
    if t == "." || t == ".." {
        return Err(format!("{t} is not a valid name."));
    }
    if t.contains('/') || t.contains('\\') || t.contains('\0') {
        return Err("Names cannot contain slashes.".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_and_parent() {
        assert_eq!(join_remote("/sdcard", "DCIM"), "/sdcard/DCIM");
        assert_eq!(join_remote("/sdcard/", "DCIM"), "/sdcard/DCIM");
        assert_eq!(join_remote("/", "sdcard"), "/sdcard");
        assert_eq!(parent_dir("/sdcard/DCIM"), "/sdcard");
        assert_eq!(parent_dir("/sdcard"), "/");
        assert_eq!(parent_dir("/"), "/");
    }

    #[test]
    fn normalize_collapses_and_resolves_dots() {
        assert_eq!(
            normalize_dir("/sdcard//DCIM/"),
            Some("/sdcard/DCIM".to_string())
        );
        assert_eq!(
            normalize_dir("/sdcard/a/../b"),
            Some("/sdcard/b".to_string())
        );
        assert_eq!(normalize_dir("/../etc"), Some("/etc".to_string()));
        assert_eq!(normalize_dir("/"), Some("/".to_string()));
        assert_eq!(normalize_dir(""), None);
        assert_eq!(normalize_dir("relative/x"), None);
    }

    #[test]
    fn mutable_gate_blocks_root_and_garbage() {
        assert!(check_mutable_path("/sdcard/x").is_ok());
        assert!(check_mutable_path("/").is_err());
        assert!(check_mutable_path("").is_err());
        assert!(check_mutable_path("relative").is_err());
    }

    #[test]
    fn child_names_validated() {
        assert!(check_child_name("New folder").is_ok());
        assert!(check_child_name("").is_err());
        assert!(check_child_name("..").is_err());
        assert!(check_child_name("a/b").is_err());
    }

    #[test]
    fn kind_from_perms() {
        assert_eq!(FileKind::from_perms("drwxr-xr-x"), FileKind::Dir);
        assert_eq!(FileKind::from_perms("-rw-rw----"), FileKind::File);
        assert_eq!(FileKind::from_perms("lrwxrwxrwx"), FileKind::Symlink);
    }
}
