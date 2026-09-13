//! Log view filtering: free-text search, minimum level, package.

use super::parser::{LogEntry, LogLevel};
use std::collections::HashMap;

/// UI-side filter state (per device buffer).
#[derive(Debug, Clone, Default)]
pub struct LogViewFilter {
    /// Substring matched against tag + message (case-insensitive).
    pub search: String,
    /// Minimum severity; `Verbose` shows everything parsed.
    pub min_level: LogLevel,
    /// Package name; empty = all packages.
    pub package: String,
}

impl LogViewFilter {
    pub fn is_default(&self) -> bool {
        self.search.is_empty() && self.min_level == LogLevel::Verbose && self.package.is_empty()
    }
}

pub fn entry_matches(
    entry: &LogEntry,
    filter: &LogViewFilter,
    pid_map: &HashMap<u32, String>,
) -> bool {
    if entry.level.rank() < filter.min_level.rank() {
        return false;
    }
    if !filter.search.is_empty() {
        let q = filter.search.to_lowercase();
        if !entry.tag.to_lowercase().contains(&q) && !entry.message.to_lowercase().contains(&q) {
            return false;
        }
    }
    if !filter.package.is_empty() {
        let pkg = filter.package.to_lowercase();
        let pid_pkg = if entry.pid != 0 {
            pid_map.get(&entry.pid).map(|p| p.to_lowercase())
        } else {
            None
        };
        if pid_pkg.as_deref() != Some(pkg.as_str())
            && !entry.tag.to_lowercase().contains(&pkg)
            && !entry.message.to_lowercase().contains(&pkg)
        {
            return false;
        }
    }
    true
}

/// Learn pid → package from ActivityManager "Start proc" lines:
/// `Start proc 12345:com.example.app/u0a152 for ...`.
pub fn learn_pid_package(message: &str) -> Option<(u32, String)> {
    let rest = message.strip_prefix("Start proc ")?;
    let (pid, pkg) = rest.split_once(':')?;
    let pid: u32 = pid.trim().parse().ok()?;
    let pkg = pkg.split('/').next()?.split_whitespace().next()?;
    if pkg.is_empty() || !pkg.contains('.') {
        return None;
    }
    Some((pid, pkg.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: LogLevel, tag: &str, message: &str, pid: u32) -> LogEntry {
        LogEntry {
            timestamp: "05-12 20:00:00.000".to_string(),
            pid,
            tid: 1,
            level,
            tag: tag.to_string(),
            message: message.to_string(),
            raw: String::new(),
            parsed: true,
        }
    }

    #[test]
    fn min_level_filters() {
        let map = HashMap::new();
        let f = LogViewFilter {
            min_level: LogLevel::Warning,
            ..Default::default()
        };
        assert!(!entry_matches(
            &entry(LogLevel::Info, "T", "m", 1),
            &f,
            &map
        ));
        assert!(entry_matches(
            &entry(LogLevel::Error, "T", "m", 1),
            &f,
            &map
        ));
        assert!(entry_matches(
            &entry(LogLevel::Fatal, "T", "m", 1),
            &f,
            &map
        ));
    }

    #[test]
    fn search_matches_tag_or_message_case_insensitive() {
        let map = HashMap::new();
        let f = LogViewFilter {
            search: "network".to_string(),
            ..Default::default()
        };
        assert!(entry_matches(
            &entry(LogLevel::Info, "NetworkManager", "up", 1),
            &f,
            &map
        ));
        assert!(entry_matches(
            &entry(LogLevel::Info, "T", "NETWORK down", 1),
            &f,
            &map
        ));
        assert!(!entry_matches(
            &entry(LogLevel::Info, "T", "hello", 1),
            &f,
            &map
        ));
    }

    #[test]
    fn package_matches_pid_map_or_text() {
        let mut map = HashMap::new();
        map.insert(1234u32, "com.example.app".to_string());
        let f = LogViewFilter {
            package: "com.example.app".to_string(),
            ..Default::default()
        };
        assert!(entry_matches(
            &entry(LogLevel::Info, "T", "m", 1234),
            &f,
            &map
        ));
        // Same package mentioned in text of an unmapped pid.
        assert!(entry_matches(
            &entry(LogLevel::Info, "T", "Start proc com.example.app", 999),
            &f,
            &map
        ));
        assert!(!entry_matches(
            &entry(LogLevel::Info, "T", "m", 999),
            &f,
            &map
        ));
    }

    #[test]
    fn learns_pid_package_from_start_proc() {
        assert_eq!(
            learn_pid_package("Start proc 12345:com.example.app/u0a152 for activity"),
            Some((12345, "com.example.app".to_string()))
        );
        assert_eq!(learn_pid_package("Start proc blah"), None);
        assert_eq!(learn_pid_package("hello"), None);
    }
}
