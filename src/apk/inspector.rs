//! APK inspection: ZIP listing + manifest decode + signature summary.
//!
//! Entry point [`inspect_apk_file`] maps every failure to
//! [`AdbError::InvalidApk`] with a human-readable reason. The pure-bytes
//! variant [`inspect_apk_bytes`] is unit-testable without touching the disk.

use super::certificate::{self, ApkSignatures};
use super::manifest::{self, ManifestData};
use super::permissions::describe_all;
use crate::adb::AdbError;
use std::io::{Cursor, Read};
use std::path::Path;

/// One ZIP entry (files only; directories are folded into counts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntryInfo {
    pub name: String,
    pub size_bytes: u64,
    pub compressed_bytes: u64,
}

/// Everything the APK page shows for one file.
#[derive(Debug, Clone, Default)]
pub struct ApkInfo {
    pub file_name: String,
    pub file_size: u64,
    pub manifest: ManifestData,
    /// Pretty permission rows (dangerous first).
    pub permissions: Vec<super::permissions::PermissionInfo>,
    pub signatures: ApkSignatures,
    /// All file entries, sorted by name (cap display at ~2000 in the UI).
    pub files: Vec<ZipEntryInfo>,
    pub total_uncompressed: u64,
    /// `classes.dex`, `lib/…`, `res/…`, `assets/…` presence shortcuts.
    pub has_dex: bool,
    pub has_native_libs: bool,
    pub architectures: Vec<String>,
}

/// Read + inspect an APK from disk.
pub fn inspect_apk_file(path: &Path) -> Result<ApkInfo, AdbError> {
    let bytes = std::fs::read(path).map_err(|e| AdbError::InvalidApk {
        message: format!("Could not read {}: {e:#}", path.display()),
    })?;
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    inspect_apk_bytes(&bytes, &file_name).map_err(|message| AdbError::InvalidApk { message })
}

/// Inspect raw APK bytes under a display name.
pub fn inspect_apk_bytes(bytes: &[u8], file_name: &str) -> Result<ApkInfo, String> {
    if bytes.len() < 4 || &bytes[..2] != b"PK" {
        return Err(format!(
            "{file_name} is not an APK (missing ZIP header). Pick a .apk file."
        ));
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("{file_name} is not a readable ZIP/APK: {e}"))?;

    let mut manifest_raw: Option<Vec<u8>> = None;
    let mut sig_entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut files: Vec<ZipEntryInfo> = Vec::new();
    let mut total_uncompressed = 0u64;

    for i in 0..zip.len() {
        let mut f = zip
            .by_index(i)
            .map_err(|e| format!("Could not list APK entries: {e}"))?;
        let name = f.name().to_string();
        if f.is_dir() {
            continue;
        }
        let size = f.size();
        total_uncompressed += size;
        files.push(ZipEntryInfo {
            name: name.clone(),
            size_bytes: size,
            compressed_bytes: f.compressed_size(),
        });
        if name == "AndroidManifest.xml" {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)
                .map_err(|e| format!("Could not read AndroidManifest.xml: {e}"))?;
            manifest_raw = Some(buf);
        } else if name.starts_with("META-INF/") {
            let upper = name.to_ascii_uppercase();
            if upper.ends_with(".RSA")
                || upper.ends_with(".DSA")
                || upper.ends_with(".EC")
                || upper.ends_with(".SF")
                || upper.ends_with(".MF")
            {
                let mut buf = Vec::new();
                f.read_to_end(&mut buf)
                    .map_err(|e| format!("Could not read {name}: {e}"))?;
                sig_entries.push((name, buf));
            }
        }
    }

    let raw = manifest_raw
        .ok_or_else(|| format!("{file_name} has no AndroidManifest.xml — not a valid APK."))?;
    let manifest = manifest::parse_manifest(&raw)
        .map_err(|e| format!("Could not parse AndroidManifest.xml in {file_name}: {e}"))?;
    if manifest.package.is_empty() {
        return Err(format!("{file_name}: manifest has no package name."));
    }

    let sig_refs: Vec<(&str, &[u8])> = sig_entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let mut signatures = certificate::summarize_entries(&sig_refs);
    signatures.has_v2_block = certificate::has_apk_signing_block(bytes);

    files.sort_by(|a, b| a.name.cmp(&b.name));
    let has_dex = files.iter().any(|f| f.name.ends_with(".dex"));
    let architectures = architectures_in(&files);

    Ok(ApkInfo {
        file_name: file_name.to_string(),
        file_size: bytes.len() as u64,
        permissions: describe_all(&manifest.permissions),
        has_native_libs: !architectures.is_empty(),
        has_dex,
        architectures,
        manifest,
        signatures,
        files,
        total_uncompressed,
    })
}

fn architectures_in(files: &[ZipEntryInfo]) -> Vec<String> {
    let mut archs = Vec::new();
    for f in files {
        if let Some(rest) = f.name.strip_prefix("lib/") {
            if let Some(arch) = rest.split('/').next() {
                if !arch.is_empty() && !archs.contains(&arch.to_string()) {
                    archs.push(arch.to_string());
                }
            }
        }
    }
    archs.sort();
    archs
}

/// Human file-size formatting (`12.4 MB`, `800 KB`, `512 B`).
pub fn human_size(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal ZIP builder (stored entries only — no compression dependency).
    fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in files {
            let local_off = out.len() as u32;
            // Local file header.
            out.extend_from_slice(&0x04034B50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // stored
            out.extend_from_slice(&0u16.to_le_bytes()); // time
            out.extend_from_slice(&0u16.to_le_bytes()); // date
            out.extend_from_slice(&crc32(data).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            // Central directory record (deferred).
            central.extend_from_slice(&0x02014B50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes()); // made by
            central.extend_from_slice(&20u16.to_le_bytes()); // needed
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc32(data).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes()); // ext attrs
            central.extend_from_slice(&local_off.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_off = out.len() as u32;
        let cd_len = central.len() as u32;
        out.extend_from_slice(&central);
        // End of central directory.
        out.extend_from_slice(&0x06054B50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_len.to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        // Small table-less CRC32 (bitwise; test-sized inputs only).
        let mut crc = 0xFFFF_FFFFu32;
        for b in data {
            crc ^= *b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    const TEXT_MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest package="com.example.app" android:versionCode="7" android:versionName="1.4">
  <uses-sdk android:minSdkVersion="26" android:targetSdkVersion="34" />
  <uses-permission android:name="android.permission.INTERNET" />
  <uses-permission android:name="android.permission.CAMERA" />
  <application android:label="Example">
    <activity android:name=".MainActivity" />
  </application>
</manifest>"#;

    #[test]
    fn inspects_text_manifest_apk() {
        let apk = stored_zip(&[
            ("AndroidManifest.xml", TEXT_MANIFEST.as_bytes()),
            ("classes.dex", b"dex\n035"),
            ("lib/arm64-v8a/libx.so", b"\x7fELF"),
            ("META-INF/CERT.SF", b"sig"),
        ]);
        let info = inspect_apk_bytes(&apk, "app.apk").expect("inspects");
        assert_eq!(info.manifest.package, "com.example.app");
        assert_eq!(info.manifest.version_name.as_deref(), Some("1.4"));
        assert_eq!(info.manifest.min_sdk.as_deref(), Some("26"));
        assert!(info.has_dex);
        assert_eq!(info.architectures, vec!["arm64-v8a".to_string()]);
        // Dangerous (CAMERA) sorts before normal (INTERNET).
        assert_eq!(info.permissions[0].name, "android.permission.CAMERA");
    }

    #[test]
    fn rejects_non_zip() {
        assert!(inspect_apk_bytes(b"definitely not zip", "x.apk").is_err());
    }

    #[test]
    fn rejects_apk_without_manifest() {
        let apk = stored_zip(&[("classes.dex", b"dex")]);
        assert!(inspect_apk_bytes(&apk, "x.apk").is_err());
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2 KB");
        assert_eq!(human_size(12 * 1024 * 1024), "12.0 MB");
    }
}
