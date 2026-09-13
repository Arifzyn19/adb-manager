//! Device-tool data models + defensive parsers (Phase 10).
//!
//! Sources: `dumpsys battery`, `/proc/meminfo`, `df`, `getprop`. Every field
//! is optional-friendly: missing/odd values degrade to `None` ("—" in the
//! UI) because dumps vary across Android versions and manufacturers.

use serde::{Deserialize, Serialize};

/// Battery snapshot from `dumpsys battery`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BatteryInfo {
    /// 0–100 (from `level`; `scale` normalizes when present and != 100).
    pub level_pct: Option<u8>,
    pub status: BatteryStatus,
    pub health: BatteryHealth,
    /// Degrees Celsius (`temperature` is in tenths).
    pub temp_c: Option<f32>,
    /// Millivolts.
    pub voltage_mv: Option<u32>,
    pub technology: Option<String>,
    /// Power sources currently attached (`AC`, `USB`, `Wireless`, …).
    pub powered: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BatteryStatus {
    Charging,
    Discharging,
    NotCharging,
    Full,
    #[default]
    Unknown,
}

impl BatteryStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Charging => "Charging",
            Self::Discharging => "Discharging",
            Self::NotCharging => "Not charging",
            Self::Full => "Full",
            Self::Unknown => "Unknown",
        }
    }

    pub fn from_code(code: i32) -> Self {
        match code {
            2 => Self::Charging,
            3 => Self::Discharging,
            4 => Self::NotCharging,
            5 => Self::Full,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BatteryHealth {
    Good,
    Overheat,
    Dead,
    OverVoltage,
    Cold,
    #[default]
    Unknown,
}

impl BatteryHealth {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Good => "Good",
            Self::Overheat => "Overheat",
            Self::Dead => "Dead",
            Self::OverVoltage => "Over voltage",
            Self::Cold => "Cold",
            Self::Unknown => "Unknown",
        }
    }

    pub fn from_code(code: i32) -> Self {
        match code {
            2 => Self::Good,
            3 => Self::Overheat,
            4 => Self::Dead,
            5 => Self::OverVoltage,
            6 => Self::Cold,
            _ => Self::Unknown,
        }
    }
}

pub fn parse_dumpsys_battery(output: &str) -> BatteryInfo {
    let mut level: Option<i64> = None;
    let mut scale: Option<i64> = None;
    let mut status = BatteryStatus::Unknown;
    let mut health = BatteryHealth::Unknown;
    let mut temp_c: Option<f32> = None;
    let mut voltage_mv: Option<u32> = None;
    let mut technology: Option<String> = None;
    let mut powered = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "level" => level = value.parse().ok(),
            "scale" => scale = value.parse().ok(),
            "status" => status = BatteryStatus::from_code(value.parse().unwrap_or(1)),
            "health" => health = BatteryHealth::from_code(value.parse().unwrap_or(1)),
            "temperature" => {
                temp_c = value.parse::<i64>().ok().map(|t| t as f32 / 10.0);
            }
            "voltage" => {
                // Millivolts on modern builds; tolerate volts-as-float just in case.
                voltage_mv = value.parse::<u32>().ok().or_else(|| {
                    value.parse::<f32>().ok().map(|v| {
                        if v < 100.0 {
                            (v * 1000.0) as u32
                        } else {
                            v as u32
                        }
                    })
                });
            }
            "technology" => {
                if !value.is_empty() {
                    technology = Some(value.to_string());
                }
            }
            "AC powered" if value == "true" => {
                powered.push("AC".to_string());
            }
            "USB powered" if value == "true" => {
                powered.push("USB".to_string());
            }
            "Wireless powered" if value == "true" => {
                powered.push("Wireless".to_string());
            }
            _ => {}
        }
    }

    let level_pct = match (level, scale) {
        (Some(l), Some(s)) if s > 0 && s != 100 => Some(((l * 100) / s).clamp(0, 100) as u8),
        (Some(l), _) => Some(l.clamp(0, 100) as u8),
        _ => None,
    };

    BatteryInfo {
        level_pct,
        status,
        health,
        temp_c,
        voltage_mv,
        technology,
        powered,
    }
}

/// RAM snapshot from `/proc/meminfo` (kilobytes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemInfo {
    pub total_kb: u64,
    pub avail_kb: u64,
}

impl MemInfo {
    pub fn used_kb(&self) -> u64 {
        self.total_kb.saturating_sub(self.avail_kb)
    }

    pub fn used_pct(&self) -> Option<u8> {
        let pct = self
            .used_kb()
            .checked_mul(100)?
            .checked_div(self.total_kb)?
            .min(100);
        Some(pct as u8)
    }
}

pub fn parse_meminfo(output: &str) -> MemInfo {
    let mut total_kb = 0u64;
    let mut avail_kb: Option<u64> = None;
    let mut free_kb = 0u64;
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let (Some(key), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        match key.trim_end_matches(':') {
            "MemTotal" => total_kb = value.parse().unwrap_or(0),
            "MemAvailable" => avail_kb = Some(value.parse().unwrap_or(0)),
            "MemFree" => free_kb = value.parse().unwrap_or(0),
            _ => {}
        }
    }
    MemInfo {
        total_kb,
        // Kernels < 3.14 lack MemAvailable; MemFree is the honest fallback.
        avail_kb: avail_kb.unwrap_or(free_kb),
    }
}

/// One `df` row (bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageInfo {
    pub filesystem: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub avail_bytes: u64,
    pub use_pct: Option<u8>,
    pub mount: String,
}

impl StorageInfo {
    pub fn use_pct_or_compute(&self) -> Option<u8> {
        if let Some(p) = self.use_pct {
            return Some(p);
        }
        if self.total_bytes == 0 {
            return None;
        }
        Some((self.used_bytes * 100 / self.total_bytes).min(100) as u8)
    }
}

/// Parse `df [-h] <mounts...>` output. Handles both 1K-block and `-h`
/// human forms (`110G`, `512M`, `1.5G`); the `%` column is optional.
pub fn parse_df(output: &str) -> Vec<StorageInfo> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("Filesystem")
            || line.starts_with("df:")
            || line.starts_with("bad ")
        {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        // Filesystem Size Used Avail Use% Mounted on  (mount may contain no
        // spaces from df; overlay lines with fewer columns are skipped)
        if tokens.len() < 6 {
            continue;
        }
        let (filesystem, size_s, used_s, avail_s) = (tokens[0], tokens[1], tokens[2], tokens[3]);
        // tokens[4] is `Use%` when it ends with '%', else the mount shifted.
        let (use_pct, mount_idx) = if tokens[4].ends_with('%') {
            (tokens[4].trim_end_matches('%').parse::<u8>().ok(), 5)
        } else {
            (None, 4)
        };
        let Some(mount) = tokens.get(mount_idx..).map(|s| s.join(" ")) else {
            continue;
        };
        if mount.is_empty() {
            continue;
        }
        out.push(StorageInfo {
            filesystem: filesystem.to_string(),
            total_bytes: parse_df_size(size_s),
            used_bytes: parse_df_size(used_s),
            avail_bytes: parse_df_size(avail_s),
            use_pct,
            mount,
        });
    }
    out
}

/// `110G` / `512M` / `1.5G` / `1024` (1K blocks) → bytes.
pub fn parse_df_size(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() || s == "-" {
        return 0;
    }
    let (num, mult) = match s.chars().last() {
        Some('K') | Some('k') => (&s[..s.len() - 1], 1024u64),
        Some('M') | Some('m') => (&s[..s.len() - 1], 1024u64.pow(2)),
        Some('G') | Some('g') => (&s[..s.len() - 1], 1024u64.pow(3)),
        Some('T') | Some('t') => (&s[..s.len() - 1], 1024u64.pow(4)),
        Some(c) if c.is_ascii_digit() => (s, 1024), // plain `df`: 1K blocks
        _ => return 0,
    };
    num.parse::<f64>()
        .map(|v| (v * mult as f64) as u64)
        .unwrap_or(0)
}

/// True when `screencap -p` output looks like a PNG (magic + IEND).
pub fn looks_like_png(bytes: &[u8]) -> bool {
    bytes.len() > 32
        && &bytes[..8] == b"\x89PNG\r\n\x1a\n"
        && bytes.windows(4).any(|w| w == b"IEND")
}

/// Reboot target for the Tools page (destructive ⇒ confirmed in the UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebootMode {
    System,
    Recovery,
    Bootloader,
}

impl RebootMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::System => "Reboot",
            Self::Recovery => "Reboot to recovery",
            Self::Bootloader => "Reboot to bootloader",
        }
    }

    /// Extra `adb reboot` argument (`None` = plain reboot).
    pub fn arg(&self) -> Option<&'static str> {
        match self {
            Self::System => None,
            Self::Recovery => Some("recovery"),
            Self::Bootloader => Some("bootloader"),
        }
    }
}
/// Screenrecord device constraints surfaced in the UI (Android caps at 180 s,
/// audio is never captured, some builds need `/sdcard` writable).
pub fn recording_remote_path(stem: &str, timestamp: &str) -> String {
    let safe: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("/sdcard/Movies/{safe}_{timestamp}.mp4")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BATTERY: &str = "Current Battery Service state:\n\
        AC powered: false\n\
        USB powered: true\n\
        Wireless powered: false\n\
        status: 2\n\
        health: 2\n\
        level: 78\n\
        scale: 100\n\
        voltage: 4200\n\
        temperature: 285\n\
        technology: Li-ion\n";

    #[test]
    fn parses_battery_fields() {
        let b = parse_dumpsys_battery(BATTERY);
        assert_eq!(b.level_pct, Some(78));
        assert_eq!(b.status, BatteryStatus::Charging);
        assert_eq!(b.health, BatteryHealth::Good);
        assert_eq!(b.temp_c, Some(28.5));
        assert_eq!(b.voltage_mv, Some(4200));
        assert_eq!(b.technology.as_deref(), Some("Li-ion"));
        assert_eq!(b.powered, vec!["USB".to_string()]);
    }

    #[test]
    fn battery_missing_fields_degrade() {
        let b = parse_dumpsys_battery("Current Battery Service state:\n  status: 9\n");
        assert_eq!(b.level_pct, None);
        assert_eq!(b.status, BatteryStatus::Unknown);
        assert!(b.powered.is_empty());
    }

    #[test]
    fn battery_scale_normalizes() {
        let b = parse_dumpsys_battery("level: 39\nscale: 50\n");
        assert_eq!(b.level_pct, Some(78));
    }

    const MEMINFO: &str = "MemTotal:        7998708 kB\n\
        MemFree:          123456 kB\n\
        MemAvailable:    3200000 kB\n";

    #[test]
    fn parses_meminfo_with_available() {
        let m = parse_meminfo(MEMINFO);
        assert_eq!(m.total_kb, 7998708);
        assert_eq!(m.avail_kb, 3200000);
        assert_eq!(m.used_kb(), 7998708 - 3200000);
        assert!(m.used_pct().unwrap() > 50);
    }

    #[test]
    fn meminfo_falls_back_to_free() {
        let m = parse_meminfo("MemTotal: 1000 kB\nMemFree: 400 kB\n");
        assert_eq!(m.avail_kb, 400);
    }

    const DF_H: &str = "Filesystem      Size  Used Avail Use% Mounted on\n\
        /dev/block/dm-5 110G   71G   39G  65% /data\n\
        /dev/fuse       110G   71G   39G  65% /sdcard\n";

    #[test]
    fn parses_df_human_rows() {
        let rows = parse_df(DF_H);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].mount, "/data");
        assert_eq!(rows[0].use_pct, Some(65));
        assert_eq!(rows[0].total_bytes, 110 * 1024u64.pow(3));
        assert_eq!(rows[1].mount, "/sdcard");
    }

    #[test]
    fn df_sizes() {
        assert_eq!(parse_df_size("110G"), 110 * 1024u64.pow(3));
        assert_eq!(parse_df_size("512M"), 512 * 1024u64.pow(2));
        assert_eq!(parse_df_size("1.5G"), (1.5 * 1024f64.powi(3)) as u64);
        assert_eq!(parse_df_size("1024"), 1024 * 1024);
        assert_eq!(parse_df_size("-"), 0);
    }

    #[test]
    fn png_magic_check() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&[0u8; 32]);
        png.extend_from_slice(b"IEND");
        assert!(looks_like_png(&png));
        assert!(!looks_like_png(b"not a png at all, way too short!!"));
    }

    #[test]
    fn recording_path_is_safe() {
        let p = recording_remote_path("my phone!", "20240512_204231");
        assert_eq!(p, "/sdcard/Movies/my_phone__20240512_204231.mp4");
    }
}
