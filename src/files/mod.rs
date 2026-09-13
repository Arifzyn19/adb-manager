//! files feature module (Phase 8).
//!
//! Android storage browser rooted at `/sdcard/` by default: `ls -la`
//! listings parsed defensively (`parser`), mutations gated against
//! root/empty paths (`entry`), transfers via `adb push`/`pull` (`manager`).
//! The UI never spawns ADB directly — everything goes through `AdbClient`.

pub mod entry;
pub mod manager;
pub mod parser;

pub use entry::{check_child_name, check_mutable_path, join_remote, normalize_dir, parent_dir};
pub use entry::{FileEntry, FileKind};
pub use manager::{delete_path, download_entry, list_dir, make_dir, rename_path, upload_files};
pub use parser::parse_ls_long;
