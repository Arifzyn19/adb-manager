//! Wireless-Debugging QR handling.
//!
//! Android shows a code of the form:
//! `WIFI:T:ADB;S:<device-name>;P:<pairing-password>;;`
//! The password becomes the `adb pair` code; the IP + pairing port are shown
//! on the phone screen and entered separately.

use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct WifiQr {
    /// Device name from the `S:` field (display only).
    pub device_name: String,
    /// Pairing password from the `P:` field → the `adb pair` code.
    pub password: String,
}

#[derive(Debug, Error, Clone)]
pub enum QrError {
    #[error("no QR code found in this image")]
    NoQrFound,
    #[error("could not decode QR code: {0}")]
    DecodeFailed(String),
    #[error("could not read image: {0}")]
    ImageError(String),
    #[error("this is not a Wireless-Debugging code: {0}")]
    NotWirelessQr(String),
    #[error("QR code has no pairing password (P: field missing)")]
    MissingPassword,
}

impl QrError {
    /// Short guidance for the UI.
    pub fn guidance(&self) -> &'static str {
        match self {
            Self::NoQrFound => "Hold the phone QR code steady inside the preview area.",
            Self::DecodeFailed(_) => {
                "The code was found but unreadable. Steady the phone and retry."
            }
            Self::ImageError(_) => "That file is not a readable PNG/JPEG image.",
            Self::NotWirelessQr(_) => {
                "Use the QR from Developer options → Wireless debugging → Pair with QR code."
            }
            Self::MissingPassword => {
                "The scanned code has no pairing password. Re-open the phone dialog."
            }
        }
    }
}

/// Parse a decoded QR string into [`WifiQr`].
///
/// Handles the standard escapes (`\;` `\:` `\,` `\"` `\\`) inside values and
/// requires the `T:ADB` marker so random Wi-Fi QR codes are rejected with a
/// clear error instead of a confusing pairing failure.
pub fn parse_wireless_qr(text: &str) -> Result<WifiQr, QrError> {
    let text = text.trim();
    let body = text
        .strip_prefix("WIFI:")
        .ok_or_else(|| QrError::NotWirelessQr("code does not start with WIFI:".to_string()))?;

    let mut fields: Vec<(String, String)> = Vec::new();
    let mut key = String::new();
    let mut value = String::new();
    let mut reading_key = true;
    let mut chars = body.chars().peekable();
    let mut terminated = false;

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Escaped char: take literally (covers \; \: \, \" \\).
            if let Some(esc) = chars.next() {
                if reading_key {
                    key.push(esc);
                } else {
                    value.push(esc);
                }
            }
            continue;
        }
        if reading_key && c == ':' {
            reading_key = false;
            continue;
        }
        if !reading_key && c == ';' {
            // `;;` ends the payload; a single `;` ends a field.
            if chars.peek() == Some(&';') {
                terminated = true;
                break;
            }
            fields.push((std::mem::take(&mut key), std::mem::take(&mut value)));
            reading_key = true;
            continue;
        }
        if reading_key {
            key.push(c);
        } else {
            value.push(c);
        }
    }
    if !key.is_empty() || !value.is_empty() {
        fields.push((key, value));
    }
    let _ = terminated;

    let get = |name: &str| {
        fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };

    match get("T").as_deref() {
        Some("ADB") => {}
        other => {
            return Err(QrError::NotWirelessQr(format!(
                "expected T:ADB, found {}",
                other.unwrap_or("(missing)")
            )));
        }
    }

    let password = get("P")
        .filter(|p| !p.is_empty())
        .ok_or(QrError::MissingPassword)?;
    let device_name = get("S").unwrap_or_default();

    Ok(WifiQr {
        device_name,
        password,
    })
}

/// Decode the first QR code in raw image bytes (PNG/JPEG/…).
/// Pure local computation — nothing is uploaded anywhere.
pub fn decode_qr_from_image_bytes(bytes: &[u8]) -> Result<String, QrError> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| QrError::ImageError(e.to_string()))?
        .to_luma8();
    decode_first_qr(&img)
}

/// Decode the first QR code in a grayscale image.
pub fn decode_first_qr(gray: &image::GrayImage) -> Result<String, QrError> {
    let mut prepared = rqrr::PreparedImage::prepare(gray.clone());
    let grids = prepared.detect_grids();
    let grid = grids.first().ok_or(QrError::NoQrFound)?;
    grid.decode()
        .map(|(_, content)| content.to_string())
        .map_err(|e| QrError::DecodeFailed(format!("{e:?}")))
}

/// Convenience: decode bytes → require a Wireless-Debugging payload.
pub fn decode_wireless_qr(bytes: &[u8]) -> Result<WifiQr, QrError> {
    let text = decode_qr_from_image_bytes(bytes)?;
    parse_wireless_qr(&text)
}

impl fmt::Display for WifiQr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.device_name.is_empty() {
            write!(f, "wireless-debugging code")
        } else {
            write!(f, "wireless-debugging code for {}", self.device_name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_wireless_qr() {
        let qr = parse_wireless_qr("WIFI:T:ADB;S:Pixel_8;P:483920;;").unwrap();
        assert_eq!(qr.device_name, "Pixel_8");
        assert_eq!(qr.password, "483920");
    }

    #[test]
    fn parses_escaped_fields() {
        // `S:My\;Phone` → `My;Phone`, `P:ab\\cd` → `ab\cd`.
        let qr = parse_wireless_qr("WIFI:T:ADB;S:My\\;Phone;P:ab\\\\cd;;").unwrap();
        assert_eq!(qr.device_name, "My;Phone");
        assert_eq!(qr.password, "ab\\cd");
    }

    #[test]
    fn rejects_non_adb_codes() {
        let err = parse_wireless_qr("WIFI:T:WPA;S:Home;P:secret;;").unwrap_err();
        assert!(matches!(err, QrError::NotWirelessQr(_)));
        let err = parse_wireless_qr("https://example.com").unwrap_err();
        assert!(matches!(err, QrError::NotWirelessQr(_)));
    }

    #[test]
    fn rejects_missing_password() {
        let err = parse_wireless_qr("WIFI:T:ADB;S:Pixel;;").unwrap_err();
        assert!(matches!(err, QrError::MissingPassword));
    }

    #[test]
    fn garbage_bytes_are_image_error_not_panic() {
        let err = decode_qr_from_image_bytes(b"definitely not an image").unwrap_err();
        assert!(matches!(err, QrError::ImageError(_)));
    }

    #[test]
    fn qr_roundtrip_through_generated_image() {
        // Generate a real QR with the `qrcode` dev-crate, then decode it
        // with the production path. Only bytes cross the crate boundary.
        let payload = "WIFI:T:ADB;S:Test_Phone;P:123456;;";
        let code = qrcode::QrCode::new(payload.as_bytes()).unwrap();
        let gray = code
            .render::<image::Luma<u8>>()
            .min_dimensions(300, 300)
            .build();
        let dynamic = image::DynamicImage::ImageLuma8(gray);
        let mut png = Vec::new();
        dynamic
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();

        let text = decode_qr_from_image_bytes(&png).unwrap();
        assert_eq!(text, payload);
        let qr = parse_wireless_qr(&text).unwrap();
        assert_eq!(qr.password, "123456");
        assert_eq!(qr.device_name, "Test_Phone");
    }
}
