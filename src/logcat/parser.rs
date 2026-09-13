//! Threadtime logcat line parser.
//!
//! Expected format (`adb logcat -v threadtime`):
//! `05-12 20:42:31.123  1234  5678 I ActivityManager: Start proc com.x`
//! Anything else (e.g. `--------- beginning of main`) yields `None` and is
//! kept by the stream as an unparsed raw line.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum LogLevel {
    #[default]
    Unknown,
    Verbose,
    Debug,
    Info,
    Warning,
    Error,
    Fatal,
}

impl LogLevel {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'V' => Some(Self::Verbose),
            'D' => Some(Self::Debug),
            'I' => Some(Self::Info),
            'W' => Some(Self::Warning),
            'E' => Some(Self::Error),
            'F' | 'A' => Some(Self::Fatal),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Unknown => "?",
            Self::Verbose => "V",
            Self::Debug => "D",
            Self::Info => "I",
            Self::Warning => "W",
            Self::Error => "E",
            Self::Fatal => "F",
        }
    }

    pub fn full_label(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Verbose => "Verbose",
            Self::Debug => "Debug",
            Self::Info => "Info",
            Self::Warning => "Warning",
            Self::Error => "Error",
            Self::Fatal => "Fatal",
        }
    }

    /// Severity rank for min-level filtering (Unknown passes nothing).
    pub fn rank(&self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Verbose => 1,
            Self::Debug => 2,
            Self::Info => 3,
            Self::Warning => 4,
            Self::Error => 5,
            Self::Fatal => 6,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub pid: u32,
    pub tid: u32,
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
    pub raw: String,
    pub parsed: bool,
}

impl LogEntry {
    /// Unparsed marker/raw line (e.g. `beginning of …` headers).
    pub fn raw_line(raw: &str) -> Self {
        Self {
            timestamp: String::new(),
            pid: 0,
            tid: 0,
            level: LogLevel::Unknown,
            tag: String::new(),
            message: raw.to_string(),
            raw: raw.to_string(),
            parsed: false,
        }
    }

    /// Single-line rendering for export / clipboard.
    pub fn to_text(&self) -> String {
        if !self.parsed {
            return self.raw.clone();
        }
        format!(
            "{} {:>5} {:>5} {} {}: {}",
            self.timestamp,
            self.pid,
            self.tid,
            self.level.label(),
            self.tag,
            self.message
        )
    }
}

/// Parse one threadtime line. Returns `None` for non-entry lines.
pub fn parse_threadtime_line(line: &str) -> Option<LogEntry> {
    // date, time, pid, tid, level, "TAG: message"
    let mut parts = line.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    if !date.contains('-') || !time.contains(':') || !time.contains('.') {
        return None;
    }
    let pid: u32 = parts.next()?.parse().ok()?;
    let tid: u32 = parts.next()?.parse().ok()?;
    let level_token = parts.next()?;
    if level_token.len() != 1 {
        return None;
    }
    let level = LogLevel::from_char(level_token.chars().next()?)?;
    // Remainder: "TAG: message". The byte offset of the 6th token:
    let mut rest = line;
    for _ in 0..5 {
        let idx = rest.find(char::is_whitespace)?;
        rest = &rest[idx..];
        rest = rest.trim_start();
    }
    let (tag, message) = rest.split_once(':')?;
    let tag = tag.trim();
    if tag.is_empty() || tag.contains(' ') {
        return None;
    }
    Some(LogEntry {
        timestamp: format!("{date} {time}"),
        pid,
        tid,
        level,
        tag: tag.to_string(),
        message: message.trim_start().to_string(),
        raw: line.to_string(),
        parsed: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_threadtime_line() {
        let e = parse_threadtime_line(
            "05-12 20:42:31.123  1234   5678 I ActivityManager: Start proc com.example.app",
        )
        .unwrap();
        assert_eq!(e.timestamp, "05-12 20:42:31.123");
        assert_eq!(e.pid, 1234);
        assert_eq!(e.tid, 5678);
        assert_eq!(e.level, LogLevel::Info);
        assert_eq!(e.tag, "ActivityManager");
        assert_eq!(e.message, "Start proc com.example.app");
    }

    #[test]
    fn message_may_contain_colons() {
        let e = parse_threadtime_line(
            "05-12 20:42:37.007  1234   5678 E AndroidRuntime: FATAL EXCEPTION: main",
        )
        .unwrap();
        assert_eq!(e.level, LogLevel::Error);
        assert_eq!(e.tag, "AndroidRuntime");
        assert_eq!(e.message, "FATAL EXCEPTION: main");
    }

    #[test]
    fn rejects_noise_lines() {
        assert!(parse_threadtime_line("--------- beginning of main").is_none());
        assert!(parse_threadtime_line("").is_none());
        assert!(parse_threadtime_line("05-12 20:42:31.123 I NoPidHere: x").is_none());
        assert!(parse_threadtime_line("05-12 20:42:31.123 1 2 X Tag: x").is_none());
        assert!(parse_threadtime_line("05-12 20:42:31.123 1 2 I Tag With Space: x").is_none());
    }

    #[test]
    fn level_ranks_ordered() {
        assert!(LogLevel::Verbose.rank() < LogLevel::Debug.rank());
        assert!(LogLevel::Debug.rank() < LogLevel::Info.rank());
        assert!(LogLevel::Info.rank() < LogLevel::Warning.rank());
        assert!(LogLevel::Warning.rank() < LogLevel::Error.rank());
        assert!(LogLevel::Error.rank() < LogLevel::Fatal.rank());
    }
}
