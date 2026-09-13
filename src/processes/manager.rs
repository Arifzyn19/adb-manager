//! Process fetch + kill orchestration (worker threads call these).

use super::parser::{parse_ps_processes, parse_top_cpu};
use super::process::ProcessInfo;
use crate::adb::{AdbClient, AdbError};

/// `ps -A` list with best-effort CPU% attached from `top`.
pub fn fetch_processes(client: &AdbClient, serial: &str) -> Result<Vec<ProcessInfo>, AdbError> {
    if crate::device::discovery::is_mock() {
        return Ok(mock_processes());
    }
    let ps = client.shell_text(serial, &["ps", "-A"])?;
    let mut procs = parse_ps_processes(&ps);
    if procs.is_empty() {
        return Err(AdbError::ExecutionFailed {
            message: "Could not read the process list on this device.".to_string(),
            exit_code: None,
            stdout: ps,
            stderr: String::new(),
        });
    }
    // CPU is optional: older/restricted builds may lack `top`.
    if let Ok(top) = client.run(&client.builder().top_once(serial)) {
        if top.success() {
            let cpu = parse_top_cpu(&top.stdout_str());
            for p in &mut procs {
                if let Some(v) = cpu.get(&p.pid) {
                    p.cpu_pct = Some(*v);
                }
            }
        }
    }
    Ok(procs)
}

/// `kill -9 PID` via the shell. Usually denied for other UIDs — the real
/// device error is surfaced so the UI can explain it.
pub fn kill_process(client: &AdbClient, serial: &str, pid: u32) -> Result<String, AdbError> {
    let out = client.run(&client.builder().kill_pid(serial, pid))?;
    let text = format!("{}{}", out.stdout_str(), out.stderr_str());
    if out.success() && text.trim().is_empty() {
        Ok(format!("Sent SIGKILL to {pid}"))
    } else if text.to_lowercase().contains("operation not permitted")
        || text.to_lowercase().contains("permission denied")
    {
        Err(AdbError::PermissionDenied {
            message: format!("The device refused to kill PID {pid} (Operation not permitted)."),
        })
    } else {
        Err(AdbError::ExecutionFailed {
            message: format!("Killing PID {pid} failed: {}", text.trim()),
            exit_code: out.exit_code,
            stdout: out.stdout_str(),
            stderr: out.stderr_str(),
        })
    }
}

fn mock_processes() -> Vec<ProcessInfo> {
    vec![
        ProcessInfo {
            pid: 12345,
            ppid: 678,
            user: "u0_a152".to_string(),
            rss_kb: 185_344,
            name: "com.example.tiktok".to_string(),
            cpu_pct: Some(12.5),
        },
        ProcessInfo {
            pid: 234,
            ppid: 1,
            user: "system".to_string(),
            rss_kb: 96_000,
            name: "system_server".to_string(),
            cpu_pct: Some(3.1),
        },
        ProcessInfo {
            pid: 12346,
            ppid: 12345,
            user: "u0_a152".to_string(),
            rss_kb: 95_344,
            name: "com.example.tiktok:service".to_string(),
            cpu_pct: Some(0.4),
        },
        ProcessInfo {
            pid: 1,
            ppid: 0,
            user: "root".to_string(),
            rss_kb: 2_400,
            name: "init".to_string(),
            cpu_pct: Some(0.1),
        },
    ]
}
