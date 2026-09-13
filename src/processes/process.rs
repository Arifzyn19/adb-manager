//! Process model + sorting.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub user: String,
    pub rss_kb: u64,
    pub name: String,
    /// CPU% snapshot from `top`; `None` when the device won't report it.
    pub cpu_pct: Option<f32>,
}

impl ProcessInfo {
    /// Best-effort package derivation: app processes are named by package.
    pub fn package_guess(&self) -> Option<&str> {
        if self.name.contains('.') {
            Some(&self.name)
        } else {
            None
        }
    }

    pub fn rss_display(&self) -> String {
        if self.rss_kb >= 1024 {
            format!("{:.1} MB", self.rss_kb as f64 / 1024.0)
        } else {
            format!("{} KB", self.rss_kb)
        }
    }

    pub fn cpu_display(&self) -> String {
        match self.cpu_pct {
            Some(v) => format!("{v:.1}%"),
            None => "—".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortColumn {
    #[default]
    Pid,
    Name,
    Cpu,
    Memory,
}

impl SortColumn {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pid => "PID",
            Self::Name => "Process",
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_formatting() {
        let p = ProcessInfo {
            pid: 1,
            ppid: 0,
            user: "root".to_string(),
            rss_kb: 2048,
            name: "init".to_string(),
            cpu_pct: None,
        };
        assert_eq!(p.rss_display(), "2.0 MB");
        assert_eq!(p.cpu_display(), "—");
        assert_eq!(p.package_guess(), None);
    }

    #[test]
    fn package_guess_for_apps() {
        let p = ProcessInfo {
            pid: 1234,
            ppid: 678,
            user: "u0_a152".to_string(),
            rss_kb: 150_000,
            name: "com.example.app".to_string(),
            cpu_pct: Some(3.5),
        };
        assert_eq!(p.package_guess(), Some("com.example.app"));
        assert_eq!(p.cpu_display(), "3.5%");
    }
}
