//! Signature summary from `META-INF` entries (APK Signature Scheme v1) +
//! APK Signing Block (v2/v3) presence heuristic.
//!
//! All fields are best-effort: custom ROMs and exotic signers produce DER we
//! have never seen, so every field except fingerprints is `Option` and the UI
//! says "unavailable" instead of failing the whole inspection.

use sha2::{Digest, Sha256};

/// One signer certificate found in `META-INF`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CertSummary {
    /// e.g. `META-INF/CERT.RSA`.
    pub file: String,
    pub size_bytes: u64,
    /// Lowercase hex SHA-256 over the raw signature block file.
    pub sha256: String,
    /// Best-effort Distinguished Name pieces (`CN=…, O=…` order as found).
    pub subject: Option<String>,
    pub issuer: Option<String>,
    /// Raw `notBefore → notAfter` strings from the DER (`None` if unparseable).
    pub validity: Option<String>,
}

/// Whole-APK signature story for the Certificate tab.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApkSignatures {
    pub certs: Vec<CertSummary>,
    /// `META-INF/*.SF` / `*.MF` present → v1 (JAR signing) material exists.
    pub has_v1: bool,
    /// `APK Sig Block 42` magic found before the central directory → v2/v3.
    pub has_v2_block: bool,
    pub debuggable_note: bool,
}

/// Summarize `META-INF` signature entries. `entries` are (name, raw bytes).
pub fn summarize_entries(entries: &[(&str, &[u8])]) -> ApkSignatures {
    let mut sig = ApkSignatures::default();
    for (name, bytes) in entries {
        let upper = name.to_ascii_uppercase();
        if name.starts_with("META-INF/") && (upper.ends_with(".SF") || upper.ends_with(".MF")) {
            sig.has_v1 = true;
        }
        if name.starts_with("META-INF/")
            && (upper.ends_with(".RSA")
                || upper.ends_with(".DSA")
                || upper.ends_with(".EC")
                || upper.ends_with(".P7B")
                || upper.ends_with(".PKCS7"))
        {
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            let sha256 = hex_encode(&hasher.finalize());
            let dn = parse_pkcs7_hint(bytes);
            sig.certs.push(CertSummary {
                file: name.to_string(),
                size_bytes: bytes.len() as u64,
                sha256,
                subject: dn.subject,
                issuer: dn.issuer,
                validity: dn.validity,
            });
            sig.has_v1 = true;
        }
    }
    sig
}

/// Scan raw APK bytes for the v2/v3 signing-block magic.
/// The block sits between the ZIP data and the central directory; searching
/// the whole file is a cheap heuristic with no false *negatives*.
pub fn has_apk_signing_block(apk_bytes: &[u8]) -> bool {
    const MAGIC: &[u8] = b"APK Sig Block 42";
    if apk_bytes.len() < MAGIC.len() {
        return false;
    }
    apk_bytes.windows(MAGIC.len()).any(|w| w == MAGIC)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}

struct DnHint {
    subject: Option<String>,
    issuer: Option<String>,
    validity: Option<String>,
}

// OID tails for 2.5.4.* attribute types → short names.
fn attr_name(tail: u8) -> Option<&'static str> {
    match tail {
        0x03 => Some("CN"),
        0x04 => None, // SURNAME — rarely useful alone; skip.
        0x05 => None,
        0x06 => Some("C"),
        0x07 => Some("L"),
        0x08 => Some("ST"),
        0x0A => Some("O"),
        0x0B => Some("OU"),
        _ => None,
    }
}

/// Best-effort DN/validity scan over a PKCS#7 / X.509 DER blob.
///
/// Strategy: find `06 03 55 04 XX` OID patterns, then read the first DER
/// string (PrintableString 0x13 / UTF8String 0x0C / Teletex 0x14 / BMP 0x1E)
/// within the following 40 bytes. Validity: first two time strings
/// (UTCTime 0x17 len 13 / GeneralizedTime 0x18 len 15) that look like dates.
/// The first DN-looking group is reported as subject, the second as issuer;
/// when only one group exists both are set to it (self-signed certs).
fn parse_pkcs7_hint(der: &[u8]) -> DnHint {
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i + 4 < der.len() {
        if der[i] == 0x06 && der[i + 1] == 0x03 && der[i + 2] == 0x55 && der[i + 3] == 0x04 {
            let tail = der[i + 4];
            if let Some(attr) = attr_name(tail) {
                if let Some(v) = read_nearby_string(der, i + 5) {
                    if !v.is_empty() && v.len() <= 128 && v.chars().all(|ch| !ch.is_control()) {
                        attrs.push((attr.to_string(), v));
                    }
                }
            }
            i += 5;
        } else {
            i += 1;
        }
    }

    let mut times: Vec<String> = Vec::new();
    let mut j = 0;
    while j + 2 < der.len() && times.len() < 2 {
        let (tag, len) = (der[j], der[j + 1] as usize);
        if (tag == 0x17 && len == 13) || (tag == 0x18 && len == 15) {
            if j + 2 + len <= der.len() {
                let s = &der[j + 2..j + 2 + len];
                if s.iter().all(|b| b.is_ascii_digit() || *b == b'Z') {
                    times.push(String::from_utf8_lossy(s).into_owned());
                }
                j += 2 + len;
                continue;
            }
        }
        j += 1;
    }

    // Group consecutive attributes into DN runs separated by gaps; the two
    // largest runs are subject/issuer candidates. Simpler and good enough:
    // first half → subject, second half → issuer.
    let fmt = |pairs: &[(String, String)]| {
        pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let (subject, issuer) = if attrs.is_empty() {
        (None, None)
    } else if attrs.len() == 1 {
        let s = fmt(&attrs);
        (Some(s.clone()), Some(s))
    } else {
        let mid = (attrs.len() + 1) / 2;
        (Some(fmt(&attrs[..mid])), Some(fmt(&attrs[mid..])))
    };
    DnHint {
        subject,
        issuer,
        validity: if times.len() == 2 {
            Some(format!("{} → {}", times[0], times[1]))
        } else {
            None
        },
    }
}

fn read_nearby_string(der: &[u8], from: usize) -> Option<String> {
    let end = (from + 40).min(der.len());
    let mut k = from;
    while k + 2 < end {
        let (tag, len) = (der[k], der[k + 1] as usize);
        if matches!(tag, 0x13 | 0x0C | 0x14 | 0x1E) && len > 0 && len <= 128 {
            if k + 2 + len <= der.len() {
                let raw = &der[k + 2..k + 2 + len];
                if tag == 0x1E {
                    // BMPString: BE UTF-16.
                    if raw.len() % 2 == 0 {
                        let u: Vec<u16> = raw
                            .chunks_exact(2)
                            .map(|c| u16::from_be_bytes([c[0], c[1]]))
                            .collect();
                        return Some(String::from_utf16_lossy(&u));
                    }
                    return None;
                }
                return Some(String::from_utf8_lossy(raw).into_owned());
            }
            return None;
        }
        k += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_rsa_with_dn(cn: &str) -> Vec<u8> {
        // OID 2.5.4.3 (CN) + PrintableString, wrapped in noise.
        let mut v = vec![0x30, 0x82, 0x01, 0x00, 0xAA, 0xBB];
        v.extend_from_slice(&[0x06, 0x03, 0x55, 0x04, 0x03]);
        v.extend_from_slice(&[0x13, cn.len() as u8]);
        v.extend_from_slice(cn.as_bytes());
        // Validity: two UTCTimes.
        v.extend_from_slice(b"\x17\x0D250101000000Z");
        v.extend_from_slice(b"\x17\x0D260101000000Z");
        v
    }

    #[test]
    fn summarizes_rsa_entry() {
        let rsa = fake_rsa_with_dn("Example");
        let sig = summarize_entries(&[("META-INF/CERT.RSA", &rsa)]);
        assert!(sig.has_v1);
        assert_eq!(sig.certs.len(), 1);
        assert_eq!(sig.certs[0].sha256.len(), 64);
        assert!(sig.certs[0]
            .subject
            .as_deref()
            .unwrap_or("")
            .contains("CN=Example"));
        assert!(sig.certs[0]
            .validity
            .as_deref()
            .unwrap_or("")
            .contains("250101000000Z"));
    }

    #[test]
    fn ignores_non_signature_entries() {
        let sig = summarize_entries(&[("AndroidManifest.xml", b"data")]);
        assert!(!sig.has_v1);
        assert!(sig.certs.is_empty());
    }

    #[test]
    fn detects_signing_block_magic() {
        let mut apk = vec![0u8; 64];
        apk.extend_from_slice(b"APK Sig Block 42");
        apk.extend_from_slice(&[0u8; 16]);
        assert!(has_apk_signing_block(&apk));
        assert!(!has_apk_signing_block(&[0u8; 64]));
    }

    #[test]
    fn fingerprints_differ_per_content() {
        let a = summarize_entries(&[("META-INF/A.RSA", b"aaa")]);
        let b = summarize_entries(&[("META-INF/A.RSA", b"bbb")]);
        assert_ne!(a.certs[0].sha256, b.certs[0].sha256);
    }
}
