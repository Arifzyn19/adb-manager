//! Background logcat streaming worker + bounded ring buffer.
//!
//! The worker owns the `adb logcat` child, parses lines, runs crash
//! detection and ships ready batches to the UI. Killing the worker always
//! kills the child (§57 cleanup).

use super::crash::CrashDetector;
use super::filter::learn_pid_package;
use super::parser::{parse_threadtime_line, LogEntry};
use crate::adb::{AdbClient, ChildKiller};
use crate::events::AppEvent;
use std::collections::{HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::time::{Duration, Instant};

/// Bounded ring buffer: oldest entries are dropped first. Never unbounded.
#[derive(Debug, Clone)]
pub struct LogBuffer {
    pub entries: VecDeque<LogEntry>,
    pub capacity: usize,
    pub total_received: u64,
    pub dropped: u64,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: capacity.max(1),
            total_received: 0,
            dropped: 0,
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        self.total_received += 1;
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
            self.dropped += 1;
        }
        self.entries.push_back(entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.dropped = 0;
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
            self.dropped += 1;
        }
    }
}

/// Background `adb logcat -v threadtime` pump for one device.
pub struct LogcatWorker {
    stop: Arc<AtomicBool>,
    killer_slot: Arc<std::sync::Mutex<Option<ChildKiller>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

const BATCH_FLUSH_MS: u64 = 120;
const BATCH_MAX_LINES: usize = 500;

impl LogcatWorker {
    pub fn start(adb_path: std::path::PathBuf, serial: String, events: Sender<AppEvent>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let killer_slot = Arc::new(std::sync::Mutex::new(None));
        let stop_flag = stop.clone();
        let slot = killer_slot.clone();

        let thread = std::thread::spawn(move || {
            run_stream(adb_path, serial, events, stop_flag, slot);
        });

        Self {
            stop,
            killer_slot,
            thread: Some(thread),
        }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.killer_slot.lock() {
            if let Some(killer) = guard.take() {
                killer.kill();
            }
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for LogcatWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.killer_slot.lock() {
            if let Some(killer) = guard.take() {
                killer.kill();
            }
        }
    }
}

fn run_stream(
    adb_path: std::path::PathBuf,
    serial: String,
    events: Sender<AppEvent>,
    stop: Arc<AtomicBool>,
    killer_slot: Arc<std::sync::Mutex<Option<ChildKiller>>>,
) {
    if crate::device::discovery::is_mock() {
        run_mock_stream(serial, events, stop);
        return;
    }
    let client = AdbClient::new(adb_path);
    let reader = match client.spawn_streaming(&client.builder().logcat(&serial)) {
        Ok(r) => r,
        Err(e) => {
            let _ = events.send(AppEvent::LogcatError {
                serial,
                message: e.to_string(),
            });
            return;
        }
    };
    if let Ok(mut guard) = killer_slot.lock() {
        *guard = Some(reader.killer());
    }

    let mut detector = CrashDetector::new();
    let mut pid_map: HashMap<u32, String> = HashMap::new();
    let mut batch: Vec<LogEntry> = Vec::new();
    let mut last_flush = Instant::now();

    for line in reader.lines {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // child died or was killed
        };
        let entry = parse_threadtime_line(&line).unwrap_or_else(|| LogEntry::raw_line(&line));
        // Learn pid → package for the package filter.
        if entry.parsed && entry.tag == "ActivityManager" {
            if let Some((pid, pkg)) = learn_pid_package(&entry.message) {
                pid_map.insert(pid, pkg);
            }
        }
        if let Some(report) = detector.feed(&entry) {
            let _ = events.send(AppEvent::CrashDetected {
                serial: serial.clone(),
                report,
            });
        }
        batch.push(entry);
        if batch.len() >= BATCH_MAX_LINES
            || (last_flush.elapsed() >= Duration::from_millis(BATCH_FLUSH_MS) && !batch.is_empty())
        {
            let _ = events.send(AppEvent::LogBatch {
                serial: serial.clone(),
                entries: std::mem::take(&mut batch),
                pid_map: pid_map.clone(),
            });
            last_flush = Instant::now();
        }
    }
    if !batch.is_empty() {
        let _ = events.send(AppEvent::LogBatch {
            serial: serial.clone(),
            entries: batch,
            pid_map,
        });
    }
}

/// Deterministic fake stream for UI development (`ADB_MANAGER_MOCK=1`).
fn run_mock_stream(serial: String, events: Sender<AppEvent>, stop: Arc<AtomicBool>) {
    let script = [
        "05-12 20:42:31.123  1000   1000 I ActivityManager: Start proc 12345:com.example.app/u0a152 for activity",
        "05-12 20:42:31.200  12345 12345 D NetworkManager: Network connected",
        "05-12 20:42:32.010  12345 12360 W Choreographer: Skipped 42 frames",
        "05-12 20:42:37.007  12345 12345 E AndroidRuntime: FATAL EXCEPTION: main",
        "05-12 20:42:37.007  12345 12345 E AndroidRuntime: Process: com.example.app, PID: 12345",
        "05-12 20:42:37.007  12345 12345 E AndroidRuntime: java.lang.NullPointerException: demo crash",
        "05-12 20:42:37.007  12345 12345 E AndroidRuntime: \tat com.example.app.MainActivity.onCreate(MainActivity.java:42)",
        "05-12 20:42:37.007  12345 12345 E AndroidRuntime: \tat android.app.Activity.performCreate(Activity.java:8000)",
        "05-12 20:42:38.100  1000   1000 I ActivityManager: Process com.example.app (pid 12345) has died: fore TOP",
    ];
    let mut detector = CrashDetector::new();
    let mut pid_map: HashMap<u32, String> = HashMap::new();
    let mut batch = Vec::new();
    for line in script {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        // Space repetitions out so the UI shows streaming, not a dump.
        std::thread::sleep(Duration::from_millis(150));
        let entry = parse_threadtime_line(line).unwrap_or_else(|| LogEntry::raw_line(line));
        if entry.parsed && entry.tag == "ActivityManager" {
            if let Some((pid, pkg)) = learn_pid_package(&entry.message) {
                pid_map.insert(pid, pkg);
            }
        }
        if let Some(report) = detector.feed(&entry) {
            let _ = events.send(AppEvent::CrashDetected {
                serial: serial.clone(),
                report,
            });
        }
        batch.push(entry);
    }
    if !batch.is_empty() {
        let _ = events.send(AppEvent::LogBatch {
            serial,
            entries: batch,
            pid_map,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_evicts_oldest() {
        let mut buf = LogBuffer::new(3);
        for i in 0..5 {
            buf.push(LogEntry::raw_line(&format!("line {i}")));
        }
        assert_eq!(buf.entries.len(), 3);
        assert_eq!(buf.dropped, 2);
        assert_eq!(buf.total_received, 5);
        assert_eq!(buf.entries[0].message, "line 2");
        buf.set_capacity(2);
        assert_eq!(buf.entries.len(), 2);
        buf.clear();
        assert!(buf.entries.is_empty());
    }
}
