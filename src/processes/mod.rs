//! Running-process management (Phase 6).
//!
//! CPU/Memory values are snapshots from `ps`/`top` — approximate by nature,
//! and the UI says so instead of claiming exactness.

pub mod manager;
pub mod parser;
pub mod process;

pub use manager::{fetch_processes, kill_process};
pub use parser::{parse_ps_processes, parse_top_cpu};
pub use process::{ProcessInfo, SortColumn};
