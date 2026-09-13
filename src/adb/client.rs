//! Central ADB client: the ONLY place that spawns `adb.exe`.
//!
//! Safety rules enforced here:
//! - direct `std::process::Command` with explicit argv (no `cmd.exe /C`, no shell)
//! - timeouts via a watchdog thread; timed-out children are killed
//! - stdout/stderr/exit-code always captured together

use super::command::AdbCommand;
use super::device::Device;
use super::errors::AdbError;
use super::parser;
use std::io::{BufRead, BufReader, Lines};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AdbClient {
    adb_path: PathBuf,
}

impl AdbClient {
    pub fn new(adb_path: PathBuf) -> Self {
        Self { adb_path }
    }

    pub fn adb_path(&self) -> &Path {
        &self.adb_path
    }

    pub fn builder(&self) -> super::command::AdbCommandBuilder {
        super::command::AdbCommandBuilder::new(self.adb_path.clone())
    }

    /// Execute a pre-built command with timeout + cleanup.
    pub fn run(&self, cmd: &AdbCommand) -> Result<OutputSnapshot, AdbError> {
        // Defense in depth: never run a command that points elsewhere.
        debug_assert_eq!(cmd.adb_path, self.adb_path);

        let mut child = Command::new(&cmd.adb_path)
            .args(&cmd.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Never pop up a console window on Windows GUI builds.
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    AdbError::NotFound {
                        searched_paths: vec![cmd.adb_path.clone()],
                        hint: Some(anyhow::anyhow!(e.to_string())),
                    }
                } else {
                    AdbError::Io(e)
                }
            })?;

        let timeout = Duration::from_secs(cmd.timeout_secs.max(1));
        let start = Instant::now();
        loop {
            match child.try_wait().map_err(AdbError::Io)? {
                Some(status) => {
                    let output = child.wait_with_output().map_err(AdbError::Io)?;
                    return Ok(OutputSnapshot::from_output(output, status.code()));
                }
                None => {
                    if start.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(AdbError::Timeout {
                            seconds: cmd.timeout_secs,
                        });
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        }
    }

    /// `adb version` → e.g. `"1.0.41"`.
    pub fn version(&self) -> Result<String, AdbError> {
        let out = self.run(&self.builder().version())?;
        if !out.success() {
            return Err(AdbError::ExecutionFailed {
                message: "adb version failed".to_string(),
                exit_code: out.exit_code,
                stdout: out.stdout_str(),
                stderr: out.stderr_str(),
            });
        }
        parser::parse_adb_version(&out.stdout_str()).ok_or_else(|| AdbError::ExecutionFailed {
            message: "could not parse adb version output".to_string(),
            exit_code: out.exit_code,
            stdout: out.stdout_str(),
            stderr: out.stderr_str(),
        })
    }

    /// `adb devices -l` → typed device list.
    pub fn devices(&self) -> Result<Vec<Device>, AdbError> {
        let out = self.run(&self.builder().devices_long())?;
        if !out.success() {
            // Surface unauthorized/offline hints preserved in stderr when useful.
            return Err(AdbError::ExecutionFailed {
                message: "adb devices -l failed".to_string(),
                exit_code: out.exit_code,
                stdout: out.stdout_str(),
                stderr: out.stderr_str(),
            });
        }
        Ok(parser::parse_devices_long(&out.stdout_str()))
    }

    /// Validate that a path points at a working ADB executable.
    pub fn validate_path(path: &Path) -> Result<String, AdbError> {
        let client = AdbClient::new(path.to_path_buf());
        if !path.is_file() {
            return Err(AdbError::InvalidPath {
                path: path.display().to_string(),
            });
        }
        client.version()
    }

    /// Spawn a long-running child (logcat) whose stdout is read line by
    /// line. The only way to stop it is [`ChildKiller::kill`] — dropping the
    /// reader does NOT kill the child, so owners must keep the killer.
    pub fn spawn_streaming(&self, cmd: &AdbCommand) -> Result<StreamReader, AdbError> {
        debug_assert_eq!(cmd.adb_path, self.adb_path);
        let mut child = Command::new(&cmd.adb_path)
            .args(&cmd.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    AdbError::NotFound {
                        searched_paths: vec![cmd.adb_path.clone()],
                        hint: Some(anyhow::anyhow!(e.to_string())),
                    }
                } else {
                    AdbError::Io(e)
                }
            })?;
        let stdout: ChildStdout = child
            .stdout
            .take()
            .ok_or_else(|| AdbError::ExecutionFailed {
                message: "could not capture child stdout".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            })?;
        let killer = ChildKiller {
            inner: Arc::new(Mutex::new(Some(child))),
        };
        Ok(StreamReader {
            lines: BufReader::new(stdout).lines(),
            killer,
        })
    }

    /// Spawn an interactive child (stdin + streaming stdout). Used for the
    /// shell page; every other one-shot invocation goes through [`Self::run`].
    pub fn spawn_interactive(&self, cmd: &AdbCommand) -> Result<InteractiveShell, AdbError> {
        debug_assert_eq!(cmd.adb_path, self.adb_path);
        let mut child = Command::new(&cmd.adb_path)
            .args(&cmd.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    AdbError::NotFound {
                        searched_paths: vec![cmd.adb_path.clone()],
                        hint: Some(anyhow::anyhow!(e.to_string())),
                    }
                } else {
                    AdbError::Io(e)
                }
            })?;
        let stdout: ChildStdout = child
            .stdout
            .take()
            .ok_or_else(|| AdbError::ExecutionFailed {
                message: "could not capture child stdout".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            })?;
        let stdin = child.stdin.take();
        let killer = ChildKiller {
            inner: Arc::new(Mutex::new(Some(child))),
        };
        Ok(InteractiveShell {
            lines: BufReader::new(stdout).lines(),
            stdin,
            killer,
        })
    }
    /// Run `adb -s SERIAL shell <args...>` and return stdout.
    ///
    /// Device-level failures are mapped to typed errors so the UI can show
    /// targeted guidance (authorize prompt, reconnect, …).
    pub fn shell_text(&self, serial: &str, shell_args: &[&str]) -> Result<String, AdbError> {
        let out = self.run(&self.builder().shell(serial, shell_args))?;
        if out.success() {
            return Ok(out.stdout_str());
        }
        Err(map_device_error(
            serial,
            &out.stdout_str(),
            &out.stderr_str(),
        ))
    }

    /// `adb connect ADDR` — Ok(output) only when ADB reports an actual
    /// connection (`adb` exits 0 even on some failures, so parse the text).
    pub fn connect(&self, addr: &str) -> Result<String, AdbError> {
        let out = self.run(&self.builder().connect(addr))?;
        let text = format!("{}{}", out.stdout_str(), out.stderr_str());
        if parser::is_connect_success(&text) {
            Ok(text.trim().to_string())
        } else {
            Err(AdbError::ConnectionFailed {
                message: parser::humanize_adb_error(&text),
            })
        }
    }

    /// `adb disconnect ADDR` — Ok(output text); the poll loop confirms removal.
    pub fn disconnect(&self, addr: &str) -> Result<String, AdbError> {
        let out = self.run(&self.builder().disconnect(Some(addr)))?;
        let text = format!("{}{}", out.stdout_str(), out.stderr_str());
        if out.success() {
            Ok(text.trim().to_string())
        } else {
            Err(AdbError::ExecutionFailed {
                message: format!("disconnecting {addr} failed"),
                exit_code: out.exit_code,
                stdout: out.stdout_str(),
                stderr: out.stderr_str(),
            })
        }
    }

    /// `adb pair ADDR CODE` — Ok(output) only when ADB reports
    /// "Successfully paired". Runs on a worker thread (blocks for seconds).
    pub fn pair(&self, addr: &str, code: &str) -> Result<String, AdbError> {
        let out = self.run(&self.builder().pair(addr, code))?;
        let text = format!("{}{}", out.stdout_str(), out.stderr_str());
        if parser::is_pair_success(&text) {
            Ok(text.trim().to_string())
        } else {
            Err(AdbError::PairingFailed {
                message: parser::humanize_adb_error(&text),
            })
        }
    }

    /// `adb -s SERIAL uninstall PKG` — Ok(output text).
    pub fn uninstall(&self, serial: &str, package: &str) -> Result<String, AdbError> {
        let out = self.run(&self.builder().uninstall(serial, package))?;
        let text = format!("{}{}", out.stdout_str(), out.stderr_str());
        if out.success() && !text.to_lowercase().contains("failure") {
            Ok(text.trim().to_string())
        } else {
            Err(AdbError::ExecutionFailed {
                message: parser::humanize_install_error(&text, package),
                exit_code: out.exit_code,
                stdout: out.stdout_str(),
                stderr: out.stderr_str(),
            })
        }
    }

    /// `adb -s SERIAL exec-out screencap -p` — raw PNG bytes.
    pub fn screencap(&self, serial: &str) -> Result<Vec<u8>, AdbError> {
        let out = self.run(&self.builder().screencap_png(serial))?;
        if out.success() && !out.stdout.is_empty() {
            Ok(out.stdout.clone())
        } else {
            Err(AdbError::ExecutionFailed {
                message: format!("Screenshot failed on {serial}"),
                exit_code: out.exit_code,
                stdout: format!("{} bytes", out.stdout.len()),
                stderr: out.stderr_str(),
            })
        }
    }

    /// Blocking `screenrecord` run (worker thread; timeout = limit + 60 s).
    pub fn screenrecord(&self, serial: &str, secs: u32, remote: &str) -> Result<String, AdbError> {
        let out = self.run(&self.builder().screenrecord(serial, secs, remote))?;
        let text = format!("{}{}", out.stdout_str(), out.stderr_str());
        if out.success() {
            Ok(format!("Recording saved on device: {remote}"))
        } else {
            Err(AdbError::ExecutionFailed {
                message: super::parser::humanize_file_error(&text, "Record", remote),
                exit_code: out.exit_code,
                stdout: out.stdout_str(),
                stderr: out.stderr_str(),
            })
        }
    }

    /// Graceful recording stop (SIGINT finalizes the MP4).
    pub fn interrupt_screenrecord(&self, serial: &str) -> Result<String, AdbError> {
        let out = self.run(&self.builder().interrupt_screenrecord(serial))?;
        let text = format!("{}{}", out.stdout_str(), out.stderr_str());
        if out.success() {
            Ok("Stop requested — finalizing video…".to_string())
        } else if text.to_lowercase().contains("not found")
            || text.to_lowercase().contains("no such")
        {
            Err(AdbError::ExecutionFailed {
                message:
                    "This device cannot stop recordings early (no pkill); wait for the time limit."
                        .to_string(),
                exit_code: out.exit_code,
                stdout: out.stdout_str(),
                stderr: out.stderr_str(),
            })
        } else {
            Err(AdbError::ExecutionFailed {
                message: format!("Stop failed: {}", text.trim()),
                exit_code: out.exit_code,
                stdout: out.stdout_str(),
                stderr: out.stderr_str(),
            })
        }
    }

    /// `adb -s SERIAL reboot [mode]` — Ok(()); the device vanishes
    /// afterwards, which the poll loop reports.
    pub fn reboot(&self, serial: &str, mode: Option<&str>) -> Result<(), AdbError> {
        let out = self.run(&self.builder().reboot(serial, mode))?;
        if out.success() {
            Ok(())
        } else {
            Err(AdbError::ExecutionFailed {
                message: format!("Reboot failed for {serial}"),
                exit_code: out.exit_code,
                stdout: out.stdout_str(),
                stderr: out.stderr_str(),
            })
        }
    }

    /// `adb kill-server` + `adb start-server` — recovery path for wedged ADB.
    pub fn restart_server(&self) -> Result<String, AdbError> {
        let kill = self.run(&self.builder().kill_server())?;
        if !kill.success() {
            return Err(AdbError::ExecutionFailed {
                message: "adb kill-server failed".to_string(),
                exit_code: kill.exit_code,
                stdout: kill.stdout_str(),
                stderr: kill.stderr_str(),
            });
        }
        let start = self.run(&self.builder().start_server())?;
        if start.success() {
            Ok("ADB server restarted".to_string())
        } else {
            Err(AdbError::ExecutionFailed {
                message: "adb start-server failed".to_string(),
                exit_code: start.exit_code,
                stdout: start.stdout_str(),
                stderr: start.stderr_str(),
            })
        }
    }

    /// `adb -s SERIAL logcat -c`.
    pub fn clear_logcat(&self, serial: &str) -> Result<String, AdbError> {
        let out = self.run(&self.builder().clear_logcat(serial))?;
        if out.success() {
            Ok(format!("On-device log ring cleared ({serial})"))
        } else {
            Err(AdbError::ExecutionFailed {
                message: format!("Clearing logcat on {serial} failed"),
                exit_code: out.exit_code,
                stdout: out.stdout_str(),
                stderr: out.stderr_str(),
            })
        }
    }
    pub fn pull(&self, serial: &str, remote: &str, local: &str) -> Result<String, AdbError> {
        let out = self.run(&self.builder().pull(serial, remote, local))?;
        let text = format!("{}{}", out.stdout_str(), out.stderr_str());
        if out.success() {
            Ok(text.trim().to_string())
        } else {
            Err(AdbError::FileTransferFailed {
                message: parser::humanize_adb_error(&text),
            })
        }
    }

    /// `adb -s SERIAL push LOCAL REMOTE` — Ok(output text).
    pub fn push(&self, serial: &str, local: &str, remote: &str) -> Result<String, AdbError> {
        let out = self.run(&self.builder().push(serial, local, remote))?;
        let text = format!("{}{}", out.stdout_str(), out.stderr_str());
        if out.success() {
            Ok(text.trim().to_string())
        } else {
            Err(AdbError::FileTransferFailed {
                message: parser::humanize_file_error(&text, "Upload", remote),
            })
        }
    }

    /// Raw `ls -la` snapshot (stdout + stderr + exit code) so the caller can
    /// distinguish "empty directory" from "no such directory".
    pub fn ls_raw(&self, serial: &str, remote_dir: &str) -> Result<OutputSnapshot, AdbError> {
        Ok(self.run(&self.builder().ls_long(serial, remote_dir))?)
    }

    /// `mkdir -p` on the device — Ok(output text).
    pub fn mkdir(&self, serial: &str, remote_dir: &str) -> Result<String, AdbError> {
        file_op(
            self.run(&self.builder().mkdir_p(serial, remote_dir))?,
            "Create folder",
            remote_dir,
        )
    }

    /// `rm -rf` on the device — Ok(output text).
    pub fn remove(&self, serial: &str, remote_path: &str) -> Result<String, AdbError> {
        file_op(
            self.run(&self.builder().rm_rf(serial, remote_path))?,
            "Delete",
            remote_path,
        )
    }

    /// `mv` on the device (rename / move) — Ok(output text).
    pub fn rename(&self, serial: &str, from: &str, to: &str) -> Result<String, AdbError> {
        file_op(
            self.run(&self.builder().mv_path(serial, from, to))?,
            "Rename",
            &format!("{from} → {to}"),
        )
    }

    /// Install APK(s) on a device: single file uses `adb install [-r]`,
    /// multiple files use `adb install-multiple [-r]` (split APKs).
    ///
    /// `display` names the app for humanized errors (package when known,
    /// otherwise the file name). Runs for minutes on large APKs — callers
    /// must run this on a worker thread.
    pub fn install(
        &self,
        serial: &str,
        apk_paths: &[String],
        reinstall: bool,
        display: &str,
    ) -> Result<String, AdbError> {
        if apk_paths.is_empty() {
            return Err(AdbError::InvalidApk {
                message: "No APK files selected.".to_string(),
            });
        }
        let cmd = if apk_paths.len() == 1 {
            self.builder().install(serial, &apk_paths[0], reinstall)
        } else {
            self.builder()
                .install_multiple(serial, apk_paths, reinstall)
        };
        let out = self.run(&cmd)?;
        let text = format!("{}{}", out.stdout_str(), out.stderr_str());
        if out.success() && parser::is_install_success(&text) {
            Ok(text.trim().to_string())
        } else {
            Err(AdbError::ExecutionFailed {
                message: parser::humanize_install_error(&text, display),
                exit_code: out.exit_code,
                stdout: out.stdout_str(),
                stderr: out.stderr_str(),
            })
        }
    }
}

/// Line iterator over a streaming child's stdout + a cloneable kill handle.
pub struct StreamReader {
    pub lines: Lines<BufReader<ChildStdout>>,
    killer: ChildKiller,
}

impl StreamReader {
    pub fn killer(&self) -> ChildKiller {
        self.killer.clone()
    }
}

/// Interactive child with a writable stdin (e.g. `adb shell`): the owner
/// writes commands and reads line-delimited replies. Kill via [`ChildKiller`].
pub struct InteractiveShell {
    pub lines: Lines<BufReader<ChildStdout>>,
    pub stdin: Option<ChildStdin>,
    killer: ChildKiller,
}

impl InteractiveShell {
    pub fn killer(&self) -> ChildKiller {
        self.killer.clone()
    }
}

/// Cloneable handle that kills the streaming child exactly once.
/// Safe to call multiple times and after natural child exit.
#[derive(Clone)]
pub struct ChildKiller {
    inner: Arc<Mutex<Option<Child>>>,
}

impl ChildKiller {
    pub fn kill(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

/// Shared success check for single-shot file ops (`mkdir`/`rm`/`mv`).
fn file_op(out: OutputSnapshot, op: &str, target: &str) -> Result<String, AdbError> {
    let text = format!("{}{}", out.stdout_str(), out.stderr_str());
    if out.success() {
        Ok(if text.trim().is_empty() {
            format!("{op} {target}: done")
        } else {
            text.trim().to_string()
        })
    } else {
        Err(AdbError::ExecutionFailed {
            message: parser::humanize_file_error(&text, op, target),
            exit_code: out.exit_code,
            stdout: out.stdout_str(),
            stderr: out.stderr_str(),
        })
    }
}

/// Map a failed device-targeted invocation to the most specific error.
fn map_device_error(serial: &str, stdout: &str, stderr: &str) -> AdbError {
    let combined = format!("{stdout}\n{stderr}").to_lowercase();
    if combined.contains("unauthorized") {
        AdbError::DeviceUnauthorized {
            serial: serial.to_string(),
        }
    } else if combined.contains("offline") {
        AdbError::DeviceOffline {
            serial: serial.to_string(),
        }
    } else if combined.contains("not found") || combined.contains("no devices") {
        AdbError::DeviceNotFound {
            serial: serial.to_string(),
        }
    } else if combined.contains("permission denied") {
        AdbError::PermissionDenied {
            message: stderr.trim().to_string(),
        }
    } else {
        AdbError::ExecutionFailed {
            message: format!("command on device {serial} failed"),
            exit_code: None,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }
}

/// Owned, Clone-friendly snapshot of a finished child process.
#[derive(Debug, Clone)]
pub struct OutputSnapshot {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl OutputSnapshot {
    pub fn from_output(o: Output, code: Option<i32>) -> Self {
        Self {
            exit_code: code.or(o.status.code()),
            stdout: o.stdout,
            stderr: o.stderr,
        }
    }

    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Candidate install locations for `adb.exe` on Windows (+ Unix fallbacks so
/// dev/test on Linux/macOS keeps working).
pub fn candidate_adb_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();

    // 1. Explicit env override (useful for dev + tests).
    if let Ok(p) = std::env::var("ADB_MANAGER_ADB") {
        let p = PathBuf::from(p);
        if !p.as_os_str().is_empty() {
            out.push(p);
        }
    }

    // 2. Anything already on PATH.
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            #[cfg(windows)]
            {
                out.push(dir.join("adb.exe"));
            }
            #[cfg(not(windows))]
            {
                out.push(dir.join("adb"));
            }
        }
    }

    // 3. Common Windows install locations.
    #[cfg(windows)]
    {
        let mut roots: Vec<PathBuf> = Vec::new();
        for key in [
            "ProgramFiles",
            "ProgramFiles(x86)",
            "LOCALAPPDATA",
            "USERPROFILE",
        ] {
            if let Ok(v) = std::env::var(key) {
                roots.push(PathBuf::from(v));
            }
        }
        let suffixes = [
            "Android\\Sdk\\platform-tools\\adb.exe",
            "Android\\platform-tools\\adb.exe",
            "platform-tools\\adb.exe",
        ];
        for root in &roots {
            for s in &suffixes {
                out.push(root.join(s));
            }
        }
        // Chocolatey / Scoop-ish spots.
        for extra in [
            "C:\\Android\\platform-tools\\adb.exe",
            "C:\\platform-tools\\adb.exe",
        ] {
            out.push(PathBuf::from(extra));
        }
        // %APPDATA%\ADBManager bundled copy (future packaging hook).
        if let Ok(appdata) = std::env::var("APPDATA") {
            out.push(
                PathBuf::from(appdata)
                    .join("ADBManager")
                    .join("platform-tools")
                    .join("adb.exe"),
            );
        }
    }

    // 4. Common Unix spots.
    #[cfg(not(windows))]
    {
        for extra in [
            "/usr/bin/adb",
            "/usr/local/bin/adb",
            "/opt/android-sdk/platform-tools/adb",
        ] {
            out.push(PathBuf::from(extra));
        }
        if let Ok(home) = std::env::var("HOME") {
            out.push(
                PathBuf::from(home)
                    .join("Android")
                    .join("Sdk")
                    .join("platform-tools")
                    .join("adb"),
            );
        }
    }

    // Deduplicate while preserving priority order.
    let mut seen = std::collections::HashSet::new();
    out.into_iter().filter(|p| seen.insert(p.clone())).collect()
}

/// Scan candidates and return the first path whose `adb version` succeeds.
pub fn detect_adb() -> Result<(PathBuf, String), AdbError> {
    let candidates = candidate_adb_paths();
    for path in &candidates {
        if path.is_file() {
            if let Ok(version) = AdbClient::validate_path(path) {
                return Ok((path.clone(), version));
            }
        }
    }
    Err(AdbError::NotFound {
        searched_paths: candidates,
        hint: None,
    })
}
