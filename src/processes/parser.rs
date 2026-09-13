//! Parsers for `ps -A` (authoritative list) and `top -n 1 -b` (CPU%).
//!
//! Column positions are derived from each header row instead of hardcoded,
//! because toybox/busybox layouts differ across Android builds.

use super::process::ProcessInfo;
use std::collections::HashMap;

/// Parse `ps -A` output. Header must contain PID + NAME (or COMMAND).
pub fn parse_ps_processes(output: &str) -> Vec<ProcessInfo> {
    let mut procs = Vec::new();
    let mut cols: Option<PsCols> = None;

    for line in output.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if cols.is_none() {
            if let Some(c) = ps_header(line) {
                cols = Some(c);
            }
            continue;
        }
        let c = cols.as_ref().unwrap();
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let get = |idx: usize| tokens.get(idx).copied().unwrap_or("");
        let Ok(pid) = get(c.pid).parse::<u32>() else {
            continue;
        };
        // NAME is the last column; join from its index (names have no spaces,
        // but WCHAN quirks make tail-joining safer).
        let name = tokens.get(c.name..).unwrap_or(&[]).join(" ");
        if name.is_empty() {
            continue;
        }
        procs.push(ProcessInfo {
            pid,
            ppid: get(c.ppid).parse().unwrap_or(0),
            user: get(c.user).to_string(),
            rss_kb: get(c.rss).replace(',', "").parse().unwrap_or(0),
            name,
            cpu_pct: None,
        });
    }
    procs
}

struct PsCols {
    user: usize,
    pid: usize,
    ppid: usize,
    rss: usize,
    name: usize,
}

fn ps_header(line: &str) -> Option<PsCols> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let find = |names: &[&str]| {
        tokens
            .iter()
            .position(|t| names.iter().any(|n| t.eq_ignore_ascii_case(n)))
    };
    let pid = find(&["PID"])?;
    // NAME may be labeled COMMAND on some builds.
    let name = find(&["NAME", "COMMAND", "ARGS", "CMD"])?;
    Some(PsCols {
        user: find(&["USER", "UID"]).unwrap_or(usize::MAX),
        pid,
        ppid: find(&["PPID"]).unwrap_or(usize::MAX),
        rss: find(&["RSS", "RES"]).unwrap_or(usize::MAX),
        name,
    })
}

/// Parse `top -n 1 -b` output into pid → CPU%. Empty map when the format is
/// unrecognized (caller shows CPU as unavailable, not as zero).
pub fn parse_top_cpu(output: &str) -> HashMap<u32, f32> {
    let mut map = HashMap::new();
    let mut cols: Option<(usize, usize)> = None; // (pid_idx, cpu_idx)

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if cols.is_none() {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            let pid = tokens.iter().position(|t| t.eq_ignore_ascii_case("PID"));
            let cpu = tokens.iter().position(|t| {
                let u = t.to_uppercase();
                u == "CPU%" || u == "%CPU" || u == "CPU"
            });
            if let (Some(p), Some(c)) = (pid, cpu) {
                cols = Some((p, c));
            }
            continue;
        }
        let (pid_idx, cpu_idx) = cols.unwrap();
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let (Some(pid_tok), Some(cpu_tok)) = (tokens.get(pid_idx), tokens.get(cpu_idx)) else {
            continue;
        };
        if let (Ok(pid), Ok(cpu)) = (
            pid_tok.parse::<u32>(),
            cpu_tok.trim_end_matches('%').parse::<f32>(),
        ) {
            map.insert(pid, cpu);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS_SAMPLE: &str =
        "USER           PID  PPID     VSZ    RSS WCHAN            ADDR S NAME\n\
        root             1      0   15000   2400 -                0 S init\n\
        u0_a152      12345    678 8000000 185344 -                0 S com.example.app\n\
        u0_a152      12346  12345 8000000  95344 -                0 S com.example.app:service\n";

    #[test]
    fn parses_ps_table() {
        let procs = parse_ps_processes(PS_SAMPLE);
        assert_eq!(procs.len(), 3);
        let app = procs.iter().find(|p| p.pid == 12345).unwrap();
        assert_eq!(app.name, "com.example.app");
        assert_eq!(app.ppid, 678);
        assert_eq!(app.rss_kb, 185344);
        assert_eq!(app.user, "u0_a152");
    }

    #[test]
    fn ps_without_header_gives_nothing() {
        assert!(parse_ps_processes("garbage\nmore garbage\n").is_empty());
    }

    const TOP_SAMPLE: &str = "Mem: 8000000K total\n\
        CPU: 5% usr 3% sys\n\
        PID PPID PR CPU% S #THR VSS RSS PCY UID Name\n\
        12345 678 10 12.5% R 120 8000000K 185344K fg u0_a152 com.example.app\n\
        1 0 20 0.3% S 2 15000K 2400K fg root init\n";

    #[test]
    fn parses_top_cpu_by_header_index() {
        let map = parse_top_cpu(TOP_SAMPLE);
        assert_eq!(map.get(&12345), Some(&12.5));
        assert_eq!(map.get(&1), Some(&0.3));
    }

    #[test]
    fn top_without_header_gives_empty_map() {
        assert!(parse_top_cpu("nothing useful here\n").is_empty());
    }
}
