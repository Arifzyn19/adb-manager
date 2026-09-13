//! ADB command builder.
//!
//! ARCHITECTURE RULE: the UI must never construct raw ADB argument lists.
//! Every ADB invocation goes through [`AdbCommandBuilder`] (arguments) and
//! [`AdbClient`](super::client::AdbClient) (execution), so `-s <serial>`
//! targeting, timeouts and injection-safety live in exactly one place.

use std::ffi::OsString;
use std::path::PathBuf;

/// Explicit, injection-safe ADB invocation description.
#[derive(Debug, Clone)]
pub struct AdbCommand {
    pub adb_path: PathBuf,
    /// Arguments *after* the executable, e.g. `["-s", "SERIAL", "shell", ...]`.
    pub args: Vec<OsString>,
    pub timeout_secs: u64,
}

impl AdbCommand {
    pub fn new(adb_path: PathBuf) -> Self {
        Self {
            adb_path,
            args: Vec::new(),
            timeout_secs: 30,
        }
    }

    pub fn arg(mut self, a: impl Into<OsString>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

/// Builds well-formed ADB commands. All device-targeted commands take an
/// explicit `serial` so multi-device support is structural, not optional.
#[derive(Debug, Clone)]
pub struct AdbCommandBuilder {
    adb_path: PathBuf,
}

impl AdbCommandBuilder {
    pub fn new(adb_path: PathBuf) -> Self {
        Self { adb_path }
    }

    fn base(&self) -> AdbCommand {
        AdbCommand::new(self.adb_path.clone())
    }

    /// `adb version`
    pub fn version(&self) -> AdbCommand {
        self.base().arg("version").timeout_secs(10)
    }

    /// `adb devices -l`
    pub fn devices_long(&self) -> AdbCommand {
        self.base().args(["devices", "-l"]).timeout_secs(15)
    }

    /// `adb start-server`
    pub fn start_server(&self) -> AdbCommand {
        self.base().args(["start-server"]).timeout_secs(30)
    }

    /// `adb kill-server`
    pub fn kill_server(&self) -> AdbCommand {
        self.base().args(["kill-server"]).timeout_secs(15)
    }

    /// `adb -s SERIAL shell <args...>`
    pub fn shell(&self, serial: &str, shell_args: &[&str]) -> AdbCommand {
        let mut cmd = self.base().args(["-s", serial, "shell"]).timeout_secs(30);
        for a in shell_args {
            cmd = cmd.arg(*a);
        }
        cmd
    }

    /// `adb -s SERIAL get-state`
    pub fn get_state(&self, serial: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "get-state"])
            .timeout_secs(10)
    }

    /// `adb -s SERIAL shell getprop` (full property dump, parsed by device::info).
    pub fn getprop(&self, serial: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "getprop"])
            .timeout_secs(20)
    }

    /// `adb -s SERIAL shell wm size`
    pub fn wm_size(&self, serial: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "wm", "size"])
            .timeout_secs(15)
    }

    /// `adb -s SERIAL shell wm density`
    pub fn wm_density(&self, serial: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "wm", "density"])
            .timeout_secs(15)
    }

    /// `adb reconnect` — reconnect all devices (USB re-enumeration helper).
    pub fn reconnect(&self) -> AdbCommand {
        self.base().arg("reconnect").timeout_secs(30)
    }

    /// `adb connect HOST:PORT`
    pub fn connect(&self, addr: &str) -> AdbCommand {
        self.base().args(["connect", addr]).timeout_secs(20)
    }

    /// `adb disconnect [HOST:PORT]`
    pub fn disconnect(&self, addr: Option<&str>) -> AdbCommand {
        let mut cmd = self.base().arg("disconnect").timeout_secs(10);
        if let Some(a) = addr {
            cmd = cmd.arg(a);
        }
        cmd
    }

    /// `adb pair HOST:PORT CODE`
    pub fn pair(&self, addr: &str, code: &str) -> AdbCommand {
        self.base().args(["pair", addr, code]).timeout_secs(30)
    }

    /// `adb -s SERIAL shell pm list packages [--third-party|--system]`
    pub fn pm_list_packages(&self, serial: &str, filter: PackageFilter) -> AdbCommand {
        let mut cmd = self
            .base()
            .args(["-s", serial, "shell", "pm", "list", "packages"])
            .timeout_secs(30);
        match filter {
            PackageFilter::All => {}
            PackageFilter::ThirdParty => cmd = cmd.arg("--third-party"),
            PackageFilter::System => cmd = cmd.arg("--system"),
        }
        cmd
    }

    /// `adb -s SERIAL shell dumpsys package PKG`
    pub fn dumpsys_package(&self, serial: &str, package: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "dumpsys", "package", package])
            .timeout_secs(20)
    }

    /// `adb -s SERIAL shell pm path PKG`
    pub fn pm_path(&self, serial: &str, package: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "pm", "path", package])
            .timeout_secs(20)
    }

    /// `adb -s SERIAL shell monkey -p PKG -c android.intent.category.LAUNCHER 1`
    pub fn launch_app(&self, serial: &str, package: &str) -> AdbCommand {
        self.base()
            .args([
                "-s",
                serial,
                "shell",
                "monkey",
                "-p",
                package,
                "-c",
                "android.intent.category.LAUNCHER",
                "1",
            ])
            .timeout_secs(30)
    }

    /// `adb -s SERIAL shell am force-stop PKG`
    pub fn force_stop(&self, serial: &str, package: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "am", "force-stop", package])
            .timeout_secs(20)
    }

    /// `adb -s SERIAL shell pm clear [--cache-only] PKG`
    pub fn pm_clear(&self, serial: &str, package: &str, cache_only: bool) -> AdbCommand {
        let mut cmd = self
            .base()
            .args(["-s", serial, "shell", "pm", "clear"])
            .timeout_secs(60);
        if cache_only {
            cmd = cmd.arg("--cache-only");
        }
        cmd.arg(package)
    }

    /// `adb -s SERIAL uninstall PKG`
    pub fn uninstall(&self, serial: &str, package: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "uninstall", package])
            .timeout_secs(120)
    }

    /// `adb -s SERIAL pull REMOTE LOCAL`
    pub fn pull(&self, serial: &str, remote: &str, local: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "pull", remote, local])
            .timeout_secs(300)
    }

    /// `adb -s SERIAL push LOCAL REMOTE`
    pub fn push(&self, serial: &str, local: &str, remote: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "push", local, remote])
            .timeout_secs(300)
    }

    /// `adb -s SERIAL shell ps -A` (process names for the "Running" filter).
    pub fn ps_all(&self, serial: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "ps", "-A"])
            .timeout_secs(20)
    }

    /// `adb -s SERIAL shell top -n 1 -b` (CPU% snapshot, best-effort).
    pub fn top_once(&self, serial: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "top", "-n", "1", "-b"])
            .timeout_secs(30)
    }

    /// `adb -s SERIAL shell kill -9 PID`.
    pub fn kill_pid(&self, serial: &str, pid: u32) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "kill", "-9", &pid.to_string()])
            .timeout_secs(15)
    }

    /// `adb -s SERIAL install [-r] APK`.
    ///
    /// Single-file fast path. `reinstall` passes `-r` (replace existing app,
    /// keep its data). Timeouts are generous: release APKs install slowly.
    pub fn install(&self, serial: &str, apk_path: &str, reinstall: bool) -> AdbCommand {
        let mut cmd = self
            .base()
            .args(["-s", serial, "install"])
            .timeout_secs(300);
        if reinstall {
            cmd = cmd.arg("-r");
        }
        cmd.arg(apk_path)
    }

    /// `adb -s SERIAL install-multiple [-r] APK...`.
    ///
    /// Split-APK path (`base.apk` + `split_config.*.apk`). Falls back to
    /// plain `install` when only one file is given — callers should prefer
    /// [`Self::install`] for that case, but this stays correct either way.
    pub fn install_multiple(
        &self,
        serial: &str,
        apk_paths: &[String],
        reinstall: bool,
    ) -> AdbCommand {
        let mut cmd = self
            .base()
            .args(["-s", serial, "install-multiple"])
            .timeout_secs(300);
        if reinstall {
            cmd = cmd.arg("-r");
        }
        for p in apk_paths {
            cmd = cmd.arg(p.as_str());
        }
        cmd
    }

    /// `adb -s SERIAL shell` with no remote command: persistent interactive
    /// session (stdin stays open). Spawned via `AdbClient::spawn_interactive`
    /// (timeouts don't apply to interactive children).
    pub fn interactive_shell(&self, serial: &str) -> AdbCommand {
        self.base().args(["-s", serial, "shell"])
    }

    /// `adb -s SERIAL exec-out screencap -p` (binary PNG on stdout).
    pub fn screencap_png(&self, serial: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "exec-out", "screencap", "-p"])
            .timeout_secs(60)
    }

    /// `adb -s SERIAL shell screenrecord --time-limit SECS REMOTE`.
    /// Android caps the limit at 180 s; callers clamp.
    pub fn screenrecord(&self, serial: &str, secs: u32, remote: &str) -> AdbCommand {
        self.base()
            .args([
                "-s",
                serial,
                "shell",
                "screenrecord",
                "--time-limit",
                &secs.to_string(),
                remote,
            ])
            .timeout_secs((secs + 60) as u64)
    }

    /// `adb -s SERIAL shell pkill -2 screenrecord`: graceful stop so the MP4
    /// finalizes. Best-effort — minimal builds may lack `pkill`.
    pub fn interrupt_screenrecord(&self, serial: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "pkill", "-2", "screenrecord"])
            .timeout_secs(15)
    }

    /// `adb -s SERIAL reboot [recovery|bootloader]`.
    pub fn reboot(&self, serial: &str, mode: Option<&str>) -> AdbCommand {
        let mut cmd = self.base().args(["-s", serial, "reboot"]).timeout_secs(30);
        if let Some(m) = mode {
            cmd = cmd.arg(m);
        }
        cmd
    }

    /// `adb -s SERIAL logcat -c` (clear the on-device log ring).
    pub fn clear_logcat(&self, serial: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "logcat", "-c"])
            .timeout_secs(15)
    }

    /// `adb -s SERIAL logcat -v threadtime` (streamed; timeout is unused —
    /// streaming execution lives in `AdbClient::spawn_streaming`).
    pub fn logcat(&self, serial: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "logcat", "-v", "threadtime"])
    }

    /// `adb -s SERIAL shell ls -la DIR` (long listing, parsed by files::parser).
    pub fn ls_long(&self, serial: &str, remote_dir: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "ls", "-la", remote_dir])
            .timeout_secs(30)
    }

    /// `adb -s SERIAL shell mkdir -p DIR`.
    pub fn mkdir_p(&self, serial: &str, remote_dir: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "mkdir", "-p", remote_dir])
            .timeout_secs(20)
    }

    /// `adb -s SERIAL shell rm -rf PATH` (files; guarded in files::manager —
    /// never called with empty/root paths).
    pub fn rm_rf(&self, serial: &str, remote_path: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "rm", "-rf", remote_path])
            .timeout_secs(60)
    }

    /// `adb -s SERIAL shell mv FROM TO` (rename + move).
    pub fn mv_path(&self, serial: &str, from: &str, to: &str) -> AdbCommand {
        self.base()
            .args(["-s", serial, "shell", "mv", from, to])
            .timeout_secs(60)
    }
}

/// `--third-party` / `--system` selector for `pm list packages`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageFilter {
    All,
    ThirdParty,
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_commands_always_carry_serial() {
        let b = AdbCommandBuilder::new(PathBuf::from("/bin/adb"));
        let cmd = b.shell("SERIAL1", &["getprop", "ro.build.version.release"]);
        let args: Vec<String> = cmd
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "-s",
                "SERIAL1",
                "shell",
                "getprop",
                "ro.build.version.release"
            ]
        );
    }

    #[test]
    fn pair_command_shape() {
        let b = AdbCommandBuilder::new(PathBuf::from("adb"));
        let cmd = b.pair("192.168.1.5:37001", "123456");
        let args: Vec<String> = cmd
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, vec!["pair", "192.168.1.5:37001", "123456"]);
    }

    #[test]
    fn info_commands_carry_serial_and_shell() {
        let b = AdbCommandBuilder::new(PathBuf::from("adb"));
        let to_vec = |cmd: AdbCommand| {
            cmd.args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            to_vec(b.getprop("S1")),
            vec!["-s", "S1", "shell", "getprop"]
        );
        assert_eq!(
            to_vec(b.wm_size("S1")),
            vec!["-s", "S1", "shell", "wm", "size"]
        );
        assert_eq!(
            to_vec(b.wm_density("S1")),
            vec!["-s", "S1", "shell", "wm", "density"]
        );
        assert_eq!(
            to_vec(b.disconnect(Some("1.2.3.4:5555"))),
            vec!["disconnect", "1.2.3.4:5555"]
        );
    }

    #[test]
    fn package_commands_carry_serial() {
        let b = AdbCommandBuilder::new(PathBuf::from("adb"));
        let to_vec = |cmd: AdbCommand| {
            cmd.args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            to_vec(b.pm_list_packages("S1", PackageFilter::ThirdParty)),
            vec![
                "-s",
                "S1",
                "shell",
                "pm",
                "list",
                "packages",
                "--third-party"
            ]
        );
        assert_eq!(
            to_vec(b.dumpsys_package("S1", "com.x")),
            vec!["-s", "S1", "shell", "dumpsys", "package", "com.x"]
        );
        assert_eq!(
            to_vec(b.launch_app("S1", "com.x")),
            vec![
                "-s",
                "S1",
                "shell",
                "monkey",
                "-p",
                "com.x",
                "-c",
                "android.intent.category.LAUNCHER",
                "1"
            ]
        );
        assert_eq!(
            to_vec(b.uninstall("S1", "com.x")),
            vec!["-s", "S1", "uninstall", "com.x"]
        );
        assert_eq!(
            to_vec(b.pull("S1", "/a.apk", "C:\\out\\a.apk")),
            vec!["-s", "S1", "pull", "/a.apk", "C:\\out\\a.apk"]
        );
        assert_eq!(
            to_vec(b.install("S1", "C:\\out\\a.apk", false)),
            vec!["-s", "S1", "install", "C:\\out\\a.apk"]
        );
        assert_eq!(
            to_vec(b.install("S1", "C:\\out\\a.apk", true)),
            vec!["-s", "S1", "install", "-r", "C:\\out\\a.apk"]
        );
        assert_eq!(
            to_vec(b.install_multiple(
                "S1",
                &["base.apk".to_string(), "split.apk".to_string()],
                true
            )),
            vec![
                "-s",
                "S1",
                "install-multiple",
                "-r",
                "base.apk",
                "split.apk"
            ]
        );
        assert_eq!(
            to_vec(b.ls_long("S1", "/sdcard/")),
            vec!["-s", "S1", "shell", "ls", "-la", "/sdcard/"]
        );
        assert_eq!(
            to_vec(b.mkdir_p("S1", "/sdcard/New")),
            vec!["-s", "S1", "shell", "mkdir", "-p", "/sdcard/New"]
        );
        assert_eq!(
            to_vec(b.rm_rf("S1", "/sdcard/old.txt")),
            vec!["-s", "S1", "shell", "rm", "-rf", "/sdcard/old.txt"]
        );
        assert_eq!(
            to_vec(b.mv_path("S1", "/sdcard/a", "/sdcard/b")),
            vec!["-s", "S1", "shell", "mv", "/sdcard/a", "/sdcard/b"]
        );
        assert_eq!(
            to_vec(b.screencap_png("S1")),
            vec!["-s", "S1", "exec-out", "screencap", "-p"]
        );
        assert_eq!(
            to_vec(b.screenrecord("S1", 30, "/sdcard/Movies/x.mp4")),
            vec![
                "-s",
                "S1",
                "shell",
                "screenrecord",
                "--time-limit",
                "30",
                "/sdcard/Movies/x.mp4"
            ]
        );
        assert_eq!(
            to_vec(b.reboot("S1", Some("recovery"))),
            vec!["-s", "S1", "reboot", "recovery"]
        );
        assert_eq!(to_vec(b.reboot("S1", None)), vec!["-s", "S1", "reboot"]);
        assert_eq!(
            to_vec(b.clear_logcat("S1")),
            vec!["-s", "S1", "logcat", "-c"]
        );
    }
}
