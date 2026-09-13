//! shell feature module (Phase 9).
//!
//! Interactive `adb shell` sessions: a persistent child per selected device,
//! marker-delimited output blocks (`session`), worker-owned with exactly-once
//! cleanup. The UI never touches the child directly.

pub mod session;

pub use session::{mock_output, parse_line, MARK_PREFIX};
pub use session::{validate_command, wrap_command, BlockAssembler, ShellLine, ShellWorker};
