//! Interactive shell (Phase 9).
//!
//! One persistent `adb -s SERIAL shell` child per selected device. Commands
//! are wrapped with an `echo` marker carrying the exit code, so output
//! blocks are delimited without a PTY:
//!
//! ```text
//! <cmd>; echo __ADBMGR_END_<id>:$?
//! …output lines…
//! __ADBMGR_END_<id>:<code>
//! ```
//!
//! `cd` and friends persist (real shell, not one-shots). Cancellation =
//! killing the session; the next Send respawns it. No blocking work ever
//! happens on the UI thread.
//!
//! One in-flight command per session: the UI disables Send while a command
//! runs, so output blocks always belong to the latest sent command.

use crate::adb::{AdbClient, AdbError, ChildKiller};
use crate::events::AppEvent;
use std::io::Write;
use std::process::ChildStdin;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc, Mutex,
};

/// Marker prefix; full marker = `{MARK_PREFIX}<id>:{code}`.
pub const MARK_PREFIX: &str = "__ADBMGR_END_";

/// Wrap a command so its output block ends with a parseable marker.
pub fn wrap_command(cmd: &str, id: u64) -> String {
    format!("{cmd}; echo {MARK_PREFIX}{id}:$?")
}

/// Reject commands we refuse to send (empty input; NUL would corrupt the pipe).
pub fn validate_command(cmd: &str) -> Result<String, String> {
    let t = cmd.trim();
    if t.is_empty() {
        return Err("Type a command first.".to_string());
    }
    if t.contains('\0') {
        return Err("Command contains NUL.".to_string());
    }
    Ok(t.to_string())
}

#[derive(Debug, PartialEq, Eq)]
pub enum ShellLine {
    Output(String),
    EndMarker { id: u64, code: i32 },
}

/// Classify one stdout line from the shell child.
pub fn parse_line(line: &str) -> ShellLine {
    if let Some(rest) = line.strip_prefix(MARK_PREFIX) {
        if let Some((id_s, code_s)) = rest.split_once(':') {
            if let (Ok(id), Ok(code)) = (id_s.parse::<u64>(), code_s.trim().parse::<i32>()) {
                return ShellLine::EndMarker { id, code };
            }
        }
    }
    ShellLine::Output(line.to_string())
}

/// Accumulates output lines until the matching end marker arrives.
#[derive(Debug, Default)]
pub struct BlockAssembler {
    id: Option<u64>,
    lines: Vec<String>,
}

impl BlockAssembler {
    pub fn begin(&mut self, id: u64) {
        self.id = Some(id);
        self.lines.clear();
    }

    /// Feed one line. Returns the completed `(id, lines, code)` when its
    /// marker arrives (`None` for stale markers from a killed session).
    pub fn feed(&mut self, line: &str) -> Option<(u64, Vec<String>, i32)> {
        match parse_line(line) {
            ShellLine::Output(text) => {
                if self.id.is_some() {
                    self.lines.push(text);
                }
                None
            }
            ShellLine::EndMarker { id, code } => {
                if self.id == Some(id) {
                    self.id = None;
                    Some((id, std::mem::take(&mut self.lines), code))
                } else {
                    None
                }
            }
        }
    }

    pub fn pending(&self) -> bool {
        self.id.is_some()
    }
}

/// Synthesize a reply for mock mode (`ADB_MANAGER_MOCK=1`).
pub fn mock_output(cmd: &str) -> Vec<String> {
    match cmd.split_whitespace().next().unwrap_or("") {
        "getprop" => vec!["[ro.build.version.release]: [16]".to_string()],
        "echo" => vec![cmd.strip_prefix("echo ").unwrap_or("").to_string()],
        "pwd" => vec!["/".to_string()],
        _ => vec![format!("(mock) {cmd}")],
    }
}

/// Background interactive-shell owner: exactly one `adb shell` child (or a
/// mock responder), a reader thread pumping lines, and a shareable stdin
/// handle for `send()`.
pub struct ShellWorker {
    stop: Arc<AtomicBool>,
    killer: Option<ChildKiller>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    mock_tx: Option<Sender<(u64, String)>>,
    /// Latest sent command; the reader takes it when a block opens.
    current: Arc<Mutex<Option<(u64, String)>>>,
    next_id: Arc<Mutex<u64>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ShellWorker {
    pub fn start(
        adb_path: std::path::PathBuf,
        serial: String,
        events: Sender<AppEvent>,
    ) -> Result<Self, AdbError> {
        if crate::device::discovery::is_mock() {
            return Ok(Self::start_mock(serial, events));
        }
        // Spawned through the centralized client (explicit argv, no shell).
        let client = AdbClient::new(adb_path);
        let shell = client.spawn_interactive(&client.builder().interactive_shell(&serial))?;
        let killer = shell.killer();
        let mut lines = shell.lines;
        let stdin = shell.stdin;
        let stop = Arc::new(AtomicBool::new(false));
        let stdin_slot: Arc<Mutex<Option<ChildStdin>>> = Arc::new(Mutex::new(stdin));
        let current: Arc<Mutex<Option<(u64, String)>>> = Arc::new(Mutex::new(None));

        let serial_clone = serial.clone();
        let stop_flag = stop.clone();
        let killer_clone = killer.clone();
        let current_clone = current.clone();
        let thread = std::thread::spawn(move || {
            let mut assembler = BlockAssembler::default();
            let _ = events.send(AppEvent::ShellReady {
                serial: serial_clone.clone(),
            });
            for line in lines.by_ref() {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break, // child died or was killed
                };
                // Device shells emit CRLF; the marker match needs clean LF.
                let line = line.strip_suffix('\r').unwrap_or(&line);
                if !assembler.pending() {
                    match parse_line(line) {
                        ShellLine::EndMarker { id, code } => {
                            // Command with no output: attribute + emit now.
                            if let Some((pid, pcmd)) =
                                current_clone.lock().ok().and_then(|mut g| g.take())
                            {
                                let _ = pid;
                                let _ = id;
                                let _ = events.send(AppEvent::ShellOutput {
                                    serial: serial_clone.clone(),
                                    cmd: pcmd,
                                    output: Vec::new(),
                                    code,
                                });
                            }
                        }
                        ShellLine::Output(text) => {
                            // First output line opens the block for the
                            // latest sent command; pre-command chatter
                            // (motd etc.) has nothing published — dropped.
                            if let Some((id, _cmd)) =
                                current_clone.lock().ok().and_then(|g| g.clone())
                            {
                                let _ = _cmd;
                                assembler.begin(id);
                                if let Some((_id, lines, code)) = assembler.feed(&text) {
                                    let cmd = current_clone
                                        .lock()
                                        .ok()
                                        .and_then(|mut g| g.take())
                                        .map(|(_, c)| c)
                                        .unwrap_or_default();
                                    let _ = _id;
                                    let _ = events.send(AppEvent::ShellOutput {
                                        serial: serial_clone.clone(),
                                        cmd,
                                        output: lines,
                                        code,
                                    });
                                }
                            }
                        }
                    }
                    continue;
                }
                if let Some((_id, lines, code)) = assembler.feed(line) {
                    let _ = _id;
                    let cmd = current_clone
                        .lock()
                        .ok()
                        .and_then(|mut g| g.take())
                        .map(|(_, c)| c)
                        .unwrap_or_default();
                    let _ = events.send(AppEvent::ShellOutput {
                        serial: serial_clone.clone(),
                        cmd,
                        output: lines,
                        code,
                    });
                }
            }
            killer_clone.kill();
            if !stop_flag.load(Ordering::Relaxed) {
                let _ = events.send(AppEvent::ShellExited {
                    serial: serial_clone,
                    message: "Shell process ended. Send a command to start a new session."
                        .to_string(),
                });
            }
        });

        Ok(Self {
            stop,
            killer: Some(killer),
            stdin: stdin_slot,
            mock_tx: None,
            current,
            next_id: Arc::new(Mutex::new(1)),
            thread: Some(thread),
        })
    }

    fn start_mock(serial: String, events: Sender<AppEvent>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (mock_tx, mock_rx) = std::sync::mpsc::channel::<(u64, String)>();
        let stop_flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let _ = events.send(AppEvent::ShellReady {
                serial: serial.clone(),
            });
            for (_id, cmd) in mock_rx {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(80));
                let _ = events.send(AppEvent::ShellOutput {
                    serial: serial.clone(),
                    cmd: cmd.clone(),
                    output: mock_output(&cmd),
                    code: 0,
                });
            }
        });
        Self {
            stop,
            killer: None,
            stdin: Arc::new(Mutex::new(None)),
            mock_tx: Some(mock_tx),
            current: Arc::new(Mutex::new(None)),
            next_id: Arc::new(Mutex::new(1)),
            thread: Some(thread),
        }
    }

    /// Send one command; returns its id. At most one in-flight command is
    /// supported — the UI enforces this with a running flag.
    pub fn send(&self, cmd: &str) -> Result<u64, AdbError> {
        let cmd = validate_command(cmd).map_err(|message| AdbError::ExecutionFailed {
            message,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        })?;
        let id = {
            let mut guard = self.next_id.lock().map_err(|_| AdbError::ExecutionFailed {
                message: "shell id lock poisoned".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            })?;
            let id = *guard;
            *guard += 1;
            id
        };
        if let Some(tx) = &self.mock_tx {
            tx.send((id, cmd)).map_err(|_| AdbError::ExecutionFailed {
                message: "Mock shell session ended.".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            })?;
            return Ok(id);
        }
        if let Ok(mut guard) = self.current.lock() {
            *guard = Some((id, cmd.clone()));
        }
        let wrapped = wrap_command(&cmd, id);
        let mut guard = self.stdin.lock().map_err(|_| AdbError::ExecutionFailed {
            message: "shell stdin lock poisoned".to_string(),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        })?;
        let stdin = guard.as_mut().ok_or_else(|| AdbError::ExecutionFailed {
            message: "Shell session is no longer running.".to_string(),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        })?;
        writeln!(stdin, "{wrapped}").map_err(AdbError::Io)?;
        stdin.flush().map_err(AdbError::Io)?;
        Ok(id)
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(killer) = &self.killer {
            killer.kill();
        }
        if let Ok(mut guard) = self.stdin.lock() {
            guard.take();
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ShellWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(killer) = &self.killer {
            killer.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_embeds_id_and_exit_code_probe() {
        assert_eq!(
            wrap_command("ls /sdcard", 7),
            "ls /sdcard; echo __ADBMGR_END_7:$?"
        );
    }

    #[test]
    fn marker_parses_id_and_code() {
        assert_eq!(
            parse_line("__ADBMGR_END_7:0"),
            ShellLine::EndMarker { id: 7, code: 0 }
        );
        assert_eq!(
            parse_line("__ADBMGR_END_12:1"),
            ShellLine::EndMarker { id: 12, code: 1 }
        );
        // Lookalikes stay output.
        assert_eq!(
            parse_line("__ADBMGR_END_oops"),
            ShellLine::Output("__ADBMGR_END_oops".to_string())
        );
        assert_eq!(
            parse_line("total 42"),
            ShellLine::Output("total 42".to_string())
        );
    }

    #[test]
    fn assembler_collects_block_until_matching_marker() {
        let mut a = BlockAssembler::default();
        a.begin(3);
        assert!(a.feed("hello").is_none());
        assert!(a.feed("world").is_none());
        // Stale marker from another command is ignored.
        assert!(a.feed("__ADBMGR_END_9:0").is_none());
        let done = a.feed("__ADBMGR_END_3:1").expect("completes");
        assert_eq!(done.0, 3);
        assert_eq!(done.1, vec!["hello".to_string(), "world".to_string()]);
        assert_eq!(done.2, 1);
        assert!(!a.pending());
    }

    #[test]
    fn empty_command_rejected() {
        assert!(validate_command("   ").is_err());
        assert_eq!(validate_command("  ls  ").unwrap(), "ls");
    }

    #[test]
    fn mock_outputs_cover_basics() {
        assert_eq!(mock_output("echo hi"), vec!["hi".to_string()]);
        assert_eq!(mock_output("pwd"), vec!["/".to_string()]);
        assert!(mock_output("dumpsys whatever")[0].starts_with("(mock)"));
    }
}
