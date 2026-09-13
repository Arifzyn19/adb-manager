//! shell feature module (Phase 9).
//!
//! Interactive `adb shell` sessions: a persistent child per selected device,
//! marker-delimited output blocks (`session`), worker-owned with exactly-once
//! cleanup. The UI never touches the child directly.

pub mod session;

pub use session::ShellWorker;
