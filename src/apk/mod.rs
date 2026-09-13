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

pub use certificate::{ApkSignatures, CertSummary};
pub use inspector::{inspect_apk_bytes, inspect_apk_file, ApkInfo, ZipEntryInfo};
pub use manager::{install_apks, validate_apk_paths};
pub use manifest::{parse_manifest, ManifestData};
pub use permissions::{classify_permission, describe_permission, PermissionInfo};
