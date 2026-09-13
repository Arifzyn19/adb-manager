//! Realtime Logcat (Phase 5).
//!
//! - [`parser`]: threadtime line parser (`LogEntry`, `LogLevel`).
//! - [`filter`]: search / level / package matching.
//! - [`crash`]: crash-pattern detector + [`crash::CrashReport`].
//! - [`stream`]: background streaming worker + bounded [`stream::LogBuffer`].
//!
//! The worker parses and crash-scans on its thread; the UI only appends
//! ready entries to a bounded ring buffer, so 10k+ lines never freeze it.

pub mod crash;
pub mod filter;
pub mod parser;
pub mod stream;

pub use crash::{system_frame, CrashDetector, CrashReason, CrashReport};
pub use filter::{entry_matches, LogViewFilter};
pub use parser::{parse_threadtime_line, LogEntry, LogLevel};
pub use stream::{LogBuffer, LogcatWorker};
