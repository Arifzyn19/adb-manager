//! ADB layer facade.
//!
//! UI and feature code must import from here (`crate::adb::...`),
//! never shell out to `adb.exe` directly.

pub mod client;
pub mod command;
pub mod device;
pub mod errors;
pub mod parser;

pub use client::{
    candidate_adb_paths, detect_adb, AdbClient, ChildKiller, OutputSnapshot, StreamReader,
};
pub use command::{AdbCommand, AdbCommandBuilder, PackageFilter};
pub use device::{Device, DeviceState, Transport};
pub use errors::AdbError;
