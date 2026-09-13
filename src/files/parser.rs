//! Parser for `ls -la` (toybox) directory listings.
//!
//! Column positions are NOT hardcoded: the permission field identifies a row
//! and the name is everything after the date/time fields, because owner,
//! group and timestamp shapes vary across Android builds. Lines that do not
//! look like listings (`total N`, daemon noise, error text) are skipped —
//! the caller decides whether "no rows" means "empty dir" or "failure" by
//! inspecting the process exit code / stderr alongside.

use super::entry::{join_remote, FileEntry, FileKind};

/// Parse `ls -la DIR` stdout into entries with absolute `path`s under `dir`.
pub fn parse_ls_long(output: &str, dir: &str) -> Vec<FileEntry> {
    let mut out = Vec::new();
    for line in output.lines() {
        if let Some(entry) = parse_ls_line(line, dir) {
            // `.`/`..` are navigation, not content.
            if entry.name == "." || entry.name == ".." {
                continue;
            }
            out.push(entry);
        }
    }
    // Directories first, then alphabetical (case-insensitive) — matches the
    // ordering users expect from desktop file managers.
    out.sort_by(|a, b| {
        let rank = |k: FileKind| match k {
            FileKind::Dir => 0,
            _ => 1,
        };
        rank(a.kind)
            .cmp(&rank(b.kind))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

/// Parse one `ls -la` row. Returns `None` for non-row lines.
///
/// Expected shape (toybox):
/// `drwxr-xr-x 4 root sdcard_rw 4096 2024-05-01 12:00 Name with spaces`
/// Older files swap the time for a year (`... 2023 Name`); both parse.
fn parse_ls_line(line: &str, dir: &str) -> Option<FileEntry> {
    let line = line.trim_end();
    if line.trim().is_empty() || line.trim_start().starts_with("total ") {
        return None;
    }
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 7 {
        return None;
    }
    let perms = tokens[0];
    // Permission field: 10 chars starting with d/-/l/c/b/p/s.
    if perms.len() != 10
        || !matches!(
            perms.chars().next(),
            Some('d' | '-' | 'l' | 'c' | 'b' | 'p' | 's')
        )
    {
        return None;
    }
    // links + owner + group + size; size failure degrades to 0, the row survives.
    let owner = tokens.get(2).unwrap_or(&"").to_string();
    let group = tokens.get(3).unwrap_or(&"").to_string();
    let size_bytes: u64 = tokens
        .get(4)
        .unwrap_or(&"0")
        .replace(',', "")
        .parse()
        .unwrap_or(0);
    // Date/time occupy 1–2 tokens; the name is everything after.
    let (modified, name_start) = if tokens.len() >= 8 {
        (format!("{} {}", tokens[5], tokens[6]), 7)
    } else {
        (tokens[5].to_string(), 6)
    };
    let raw_name = tokens.get(name_start..)?.join(" ");
    if raw_name.is_empty() {
        return None;
    }
    let kind = FileKind::from_perms(perms);
    let (name, link_target) = match raw_name.split_once(" -> ") {
        Some((n, t)) if kind == FileKind::Symlink => (n.to_string(), Some(t.to_string())),
        _ => (raw_name, None),
    };
    let path = join_remote(dir, &name);
    Some(FileEntry {
        name,
        path,
        kind,
        size_bytes,
        perms: perms.to_string(),
        owner,
        group,
        modified,
        link_target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "total 48\n\
        drwxr-xr-x  6 root sdcard_rw 4096 2024-05-01 12:00 .\n\
        drwxr-xr-x  3 root root      4096 2024-04-30 09:11 ..\n\
        drwxr-xr-x  2 root sdcard_rw 4096 2024-05-01 12:00 DCIM\n\
        -rw-rw----  1 root sdcard_rw 123456 2024-05-01 12:01 photo 1.jpg\n\
        lrwxrwxrwx  1 root root         21 2023-01-02 2023 link -> /data/media\n\
        -rw-rw----  1 root sdcard_rw    512 2022-11-30 2022 old.bin\n";

    #[test]
    fn parses_rows_and_skips_dots_and_total() {
        let entries = parse_ls_long(SAMPLE, "/sdcard");
        assert_eq!(entries.len(), 4);
        // Directories sort first.
        assert_eq!(entries[0].name, "DCIM");
        assert_eq!(entries[0].kind, FileKind::Dir);
        assert_eq!(entries[0].path, "/sdcard/DCIM");
    }

    #[test]
    fn names_with_spaces_survive() {
        let entries = parse_ls_long(SAMPLE, "/sdcard");
        let photo = entries.iter().find(|e| e.name == "photo 1.jpg").unwrap();
        assert_eq!(photo.size_bytes, 123456);
        assert_eq!(photo.path, "/sdcard/photo 1.jpg");
    }

    #[test]
    fn symlinks_split_target() {
        let entries = parse_ls_long(SAMPLE, "/sdcard");
        let link = entries.iter().find(|e| e.name == "link").unwrap();
        assert_eq!(link.kind, FileKind::Symlink);
        assert_eq!(link.link_target.as_deref(), Some("/data/media"));
    }

    #[test]
    fn year_instead_of_time_parses() {
        let entries = parse_ls_long(SAMPLE, "/sdcard");
        let old = entries.iter().find(|e| e.name == "old.bin").unwrap();
        assert_eq!(old.size_bytes, 512);
        assert!(old.modified.contains("2022"));
    }

    #[test]
    fn garbage_lines_are_ignored() {
        let out = "daemon started\ntotal 0\nnot a listing at all\n";
        assert!(parse_ls_long(out, "/sdcard").is_empty());
        assert!(parse_ls_long("", "/sdcard").is_empty());
    }
}
