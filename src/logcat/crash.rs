//! Crash detection over the log stream + crash reports.
//!
//! Only real crash patterns create reports — ordinary error logs never do:
//! - `FATAL EXCEPTION` (+ `Process:` + exception + `at …` stack)
//! - `ANR in <package>`
//! - `Process <package> (pid N) has died`
//! - `Force finishing activity <package>/…`

use super::parser::LogEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashReason {
    FatalException,
    Anr,
    ProcessDied,
    ForceFinish,
}

impl CrashReason {
    pub fn label(&self) -> &'static str {
        match self {
            Self::FatalException => "Crash",
            Self::Anr => "ANR",
            Self::ProcessDied => "Process died",
            Self::ForceFinish => "Force finished",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CrashReport {
    pub id: u64,
    pub package: String,
    pub process: String,
    pub exception: String,
    pub thread: String,
    pub timestamp: String,
    pub reason: CrashReason,
    pub stack: Vec<String>,
}

impl CrashReport {
    pub fn short_exception(&self) -> String {
        // `java.lang.NullPointerException: msg` → `NullPointerException`.
        let head = self.exception.split(':').next().unwrap_or(&self.exception);
        head.rsplit('.').next().unwrap_or(head).to_string()
    }

    /// Stack with framework frames optionally removed (app frames kept).
    pub fn visible_stack(&self, hide_system: bool) -> Vec<&str> {
        self.stack
            .iter()
            .map(String::as_str)
            .filter(|l| !hide_system || !system_frame(l))
            .collect()
    }

    pub fn to_text(&self, hide_system: bool) -> String {
        let mut out = format!(
            "{} — {}\nPackage: {}\nProcess: {}\nThread: {}\nTime: {}\nException: {}\n\nStack trace:\n",
            self.reason.label(),
            self.short_exception(),
            self.package,
            self.process,
            self.thread,
            self.timestamp,
            self.exception,
        );
        for line in self.visible_stack(hide_system) {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

/// True for framework stack frames (`at android.…`, `at java.…`, …).
/// Application frames (`at com.example.…`) are never filtered.
pub fn system_frame(line: &str) -> bool {
    let t = line.trim_start();
    let Some(rest) = t.strip_prefix("at ") else {
        return false;
    };
    [
        "android.",
        "com.android.",
        "java.",
        "javax.",
        "kotlin.",
        "kotlinx.",
        "dalvik.",
        "libcore.",
        "sun.",
        "jdk.",
        "org.apache.harmony.",
    ]
    .iter()
    .any(|p| rest.starts_with(p))
}

/// Stateful single-device crash detector. Feed parsed entries in order;
/// returns a report the moment a crash block completes.
pub struct CrashDetector {
    next_id: u64,
    pending: Option<PendingCrash>,
}

#[derive(Debug)]
struct PendingCrash {
    thread: String,
    timestamp: String,
    package: Option<String>,
    exception: Option<String>,
    stack: Vec<String>,
}

const MAX_STACK: usize = 200;

impl CrashDetector {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            pending: None,
        }
    }

    pub fn feed(&mut self, entry: &LogEntry) -> Option<CrashReport> {
        if !entry.parsed {
            return self.feed_raw(&entry.message);
        }
        // One-line patterns complete immediately (only when not mid-crash).
        if self.pending.is_none() {
            if let Some(report) = self.check_single_line(entry) {
                return Some(report);
            }
        }
        // Multi-line FATAL EXCEPTION blocks.
        if entry.message.contains("FATAL EXCEPTION") {
            self.pending = Some(PendingCrash {
                thread: after_colon(&entry.message).unwrap_or("?").to_string(),
                timestamp: entry.timestamp.clone(),
                package: None,
                exception: None,
                stack: Vec::new(),
            });
            return None;
        }
        let Some(pending) = self.pending.as_mut() else {
            return None;
        };
        let msg = entry.message.trim();
        if let Some(rest) = msg.strip_prefix("Process: ") {
            // `Process: com.example.app, PID: 1234`
            pending.package = rest.split(',').next().map(|s| s.trim().to_string());
            return None;
        }
        if pending.exception.is_none() {
            if let Some(exc) = parse_exception_line(msg) {
                pending.exception = Some(exc);
                return None;
            }
        }
        if msg.starts_with("at ") || msg.starts_with("Caused by:") || msg.starts_with("Suppressed:")
        {
            if pending.stack.len() < MAX_STACK {
                pending.stack.push(entry.message.clone());
            }
            return None;
        }
        if msg.is_empty() {
            return self.finish(entry);
        }
        // An unrelated log line ends the block.
        if looks_like_new_entry(msg) {
            return self.finish(entry);
        }
        // Continuation detail (e.g. `... 5 more`) — keep, bounded.
        if pending.stack.len() < MAX_STACK {
            pending.stack.push(entry.message.clone());
        }
        None
    }

    /// Unparsed raw lines can only terminate a pending block.
    fn feed_raw(&mut self, message: &str) -> Option<CrashReport> {
        if message.trim().is_empty() && self.pending.is_some() {
            let dummy = LogEntry::raw_line("");
            return self.finish(&dummy);
        }
        None
    }

    fn finish(&mut self, entry: &LogEntry) -> Option<CrashReport> {
        let pending = self.pending.take()?;
        let package = pending.package.clone().unwrap_or_else(|| "?".to_string());
        Some(CrashReport {
            id: self.take_id(),
            package: package.clone(),
            process: package,
            exception: pending
                .exception
                .unwrap_or_else(|| "Unknown exception".to_string()),
            thread: pending.thread,
            timestamp: if pending.timestamp.is_empty() {
                entry.timestamp.clone()
            } else {
                pending.timestamp
            },
            reason: CrashReason::FatalException,
            stack: pending.stack,
        })
    }

    fn check_single_line(&mut self, entry: &LogEntry) -> Option<CrashReport> {
        let msg = entry.message.as_str();
        // `ANR in com.example.app`
        if let Some(idx) = msg.find("ANR in ") {
            let pkg = msg[idx + "ANR in ".len()..]
                .split([' ', ',', ';', ':'])
                .next()
                .unwrap_or("?")
                .trim();
            if !pkg.is_empty() {
                return Some(self.simple(
                    entry,
                    CrashReason::Anr,
                    pkg,
                    "Application Not Responding",
                ));
            }
        }
        // `Process com.example.app (pid 1234) has died`
        if msg.contains("has died") {
            if let Some(pkg) = process_died_package(msg) {
                return Some(self.simple(entry, CrashReason::ProcessDied, &pkg, "Process died"));
            }
        }
        // `Force finishing activity com.example.app/.Main`
        if msg.contains("Force finishing activity") {
            if let Some(pkg) = force_finish_package(msg) {
                return Some(self.simple(
                    entry,
                    CrashReason::ForceFinish,
                    &pkg,
                    "Activity force-finished",
                ));
            }
        }
        None
    }

    fn simple(
        &mut self,
        entry: &LogEntry,
        reason: CrashReason,
        package: &str,
        exception: &str,
    ) -> CrashReport {
        CrashReport {
            id: self.take_id(),
            package: package.to_string(),
            process: package.to_string(),
            exception: exception.to_string(),
            thread: String::new(),
            timestamp: entry.timestamp.clone(),
            reason,
            stack: vec![entry.message.clone()],
        }
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Default for CrashDetector {
    fn default() -> Self {
        Self::new()
    }
}

fn after_colon(s: &str) -> Option<&str> {
    s.split_once(':').map(|(_, rest)| rest.trim())
}

/// `java.lang.NullPointerException: msg` → Some(full line, trimmed).
/// Requires a dotted exception/error class name to avoid false positives.
fn parse_exception_line(msg: &str) -> Option<String> {
    let head = msg.split(':').next()?.trim();
    // Class name is the last whitespace-separated token.
    let class = head.split_whitespace().last()?;
    if !class.contains('.') {
        return None;
    }
    let short = class.rsplit('.').next()?;
    if !(short.ends_with("Exception") || short.ends_with("Error") || short.ends_with("Throwable")) {
        return None;
    }
    Some(msg.trim().to_string())
}

/// A stack block ends when a fresh log statement arrives (heuristic: a line
/// that does not look like stack continuation at all).
fn looks_like_new_entry(msg: &str) -> bool {
    // Fresh statements rarely start with whitespace in threadtime messages…
    // stack lines always do. Anything non-indented that is not a known
    // continuation header ends the block.
    !(msg.starts_with(' ') || msg.starts_with('\t'))
        && !msg.starts_with("Caused by:")
        && !msg.starts_with("Suppressed:")
        && !msg.starts_with("Process:")
        && !msg.starts_with("FATAL EXCEPTION")
}

/// `... Process com.example.app (pid 1234) has died ...` → package.
fn process_died_package(msg: &str) -> Option<String> {
    let idx = msg.find("Process ")?;
    let pkg = msg[idx + "Process ".len()..]
        .split_whitespace()
        .next()?
        .trim_end_matches([':', ',']);
    if pkg.contains('.') {
        Some(pkg.to_string())
    } else {
        None
    }
}

/// `Force finishing activity com.example.app/.MainActivity` → package.
fn force_finish_package(msg: &str) -> Option<String> {
    let idx = msg.find("Force finishing activity ")?;
    let component = msg[idx + "Force finishing activity ".len()..]
        .split_whitespace()
        .next()?;
    let pkg = component.split('/').next()?;
    if pkg.contains('.') {
        Some(pkg.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logcat::parser::parse_threadtime_line;

    fn feed_lines(detector: &mut CrashDetector, lines: &[&str]) -> Vec<CrashReport> {
        let mut reports = Vec::new();
        for line in lines {
            let entry = parse_threadtime_line(line).unwrap_or_else(|| LogEntry::raw_line(line));
            if let Some(r) = detector.feed(&entry) {
                reports.push(r);
            }
        }
        reports
    }

    const FATAL: &[&str] = &[
        "05-12 20:42:37.007  1234   1234 E AndroidRuntime: FATAL EXCEPTION: main",
        "05-12 20:42:37.007  1234   1234 E AndroidRuntime: Process: com.example.app, PID: 1234",
        "05-12 20:42:37.007  1234   1234 E AndroidRuntime: java.lang.NullPointerException: Attempt to invoke virtual method on a null object reference",
        "05-12 20:42:37.007  1234   1234 E AndroidRuntime: \tat com.example.app.MainActivity.onCreate(MainActivity.java:42)",
        "05-12 20:42:37.007  1234   1234 E AndroidRuntime: \tat android.app.Activity.performCreate(Activity.java:8000)",
        "05-12 20:42:37.007  1234   1234 E AndroidRuntime: Caused by: java.lang.IllegalStateException: bad state",
        "05-12 20:42:37.007  1234   1234 E AndroidRuntime: \tat com.example.app.Helper.init(Helper.java:7)",
        "05-12 20:42:38.100  1234   1234 I ActivityManager: Start proc com.example.app",
    ];

    #[test]
    fn detects_fatal_exception_with_stack() {
        let mut d = CrashDetector::new();
        let reports = feed_lines(&mut d, FATAL);
        assert_eq!(reports.len(), 1);
        let r = &reports[0];
        assert_eq!(r.reason, CrashReason::FatalException);
        assert_eq!(r.package, "com.example.app");
        assert_eq!(r.thread, "main");
        assert!(r.exception.contains("NullPointerException"));
        assert_eq!(r.stack.len(), 4);
        // App frames survive system-frame filtering; framework ones don't.
        let visible = r.visible_stack(true);
        assert_eq!(visible.len(), 3);
        assert!(visible.iter().all(|l| !l.contains("android.app.Activity")));
    }

    #[test]
    fn ordinary_errors_are_not_crashes() {
        let mut d = CrashDetector::new();
        let reports = feed_lines(
            &mut d,
            &[
                "05-12 20:42:31.123  1  2 E NetworkManager: Network connected failed, retrying",
                "05-12 20:42:32.123  1  2 E SensorService: sensor error 42",
                "05-12 20:42:33.123  1  2 W ActivityManager: Slow operation",
            ],
        );
        assert!(reports.is_empty());
    }

    #[test]
    fn detects_anr_process_died_force_finish() {
        let mut d = CrashDetector::new();
        let reports = feed_lines(
            &mut d,
            &[
                "05-12 21:00:00.000  500  500 E ActivityManager: ANR in com.example.app",
                "05-12 21:01:00.000  500  500 I ActivityManager: Process com.example.app (pid 1234) has died: fore TOP",
                "05-12 21:02:00.000  500  500 W ActivityManager: Force finishing activity com.example.app/.MainActivity",
            ],
        );
        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0].reason, CrashReason::Anr);
        assert_eq!(reports[1].reason, CrashReason::ProcessDied);
        assert_eq!(reports[2].reason, CrashReason::ForceFinish);
        assert!(reports.iter().all(|r| r.package == "com.example.app"));
    }

    #[test]
    fn system_frame_classification() {
        assert!(system_frame(
            "\tat android.app.Activity.performCreate(Activity.java:1)"
        ));
        assert!(system_frame("\tat java.lang.Thread.run(Thread.java:1)"));
        assert!(system_frame(
            "\tat com.android.internal.os.ZygoteInit.main(ZygoteInit.java:1)"
        ));
        assert!(!system_frame(
            "\tat com.example.app.MainActivity.onCreate(MainActivity.java:42)"
        ));
        assert!(!system_frame("FATAL EXCEPTION: main"));
    }

    #[test]
    fn crash_report_text_roundtrip() {
        let mut d = CrashDetector::new();
        let reports = feed_lines(&mut d, FATAL);
        let text = reports[0].to_text(false);
        assert!(text.contains("com.example.app"));
        assert!(text.contains("NullPointerException"));
        assert_eq!(reports[0].short_exception(), "NullPointerException");
    }
}
