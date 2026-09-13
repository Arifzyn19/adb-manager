//! apk feature module (Phase 7).
//!
//! Pure-Rust APK inspection: no `aapt2`, no downloads, no shell-outs.
//! `AndroidManifest.xml` inside an APK is binary AXML — parsed here with a
//! minimal decoder (`manifest`). Signatures are summarized from `META-INF`
//! entries (`certificate`); installs go through `AdbClient::install`.

pub mod certificate;
pub mod inspector;
pub mod manager;
pub mod manifest;
pub mod permissions;

pub use inspector::{ApkInfo, ZipEntryInfo};
pub use manager::{inspect_apk, install_apks};
