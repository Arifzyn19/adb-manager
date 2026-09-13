//! Pairing-code flow: request validation + pairing state machine.
//!
//! The pairing port (shown under "Pair device with pairing code") is NOT the
//! ADB connection port (shown on the main Wireless debugging screen). The UI
//! must always label which one it wants.

use serde::{Deserialize, Serialize};

/// Validated `adb pair IP:PORT CODE` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingRequest {
    pub ip: String,
    pub port: u16,
    pub code: String,
}

impl PairingRequest {
    pub fn addr(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PairingState {
    #[default]
    Idle,
    Pairing,
    Paired,
    Failed,
}

impl PairingState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Not paired",
            Self::Pairing => "Pairing…",
            Self::Paired => "Paired",
            Self::Failed => "Pairing failed",
        }
    }

    pub fn in_progress(&self) -> bool {
        matches!(self, Self::Pairing)
    }
}

/// Validate raw Wireless-tab input. Returns the request or a message
/// suitable for showing next to the form.
pub fn validate_pair_input(ip: &str, port: &str, code: &str) -> Result<PairingRequest, String> {
    let ip = ip.trim();
    if ip.is_empty() {
        return Err("Enter the IP address shown on the phone.".to_string());
    }
    // Basic sanity: IPv4 dotted quad or hostname/IPv6.
    let ip_ok = ip.contains('.') || ip.contains(':') || !ip.contains(' ');
    if !ip_ok {
        return Err("That IP address does not look valid.".to_string());
    }

    let port: u16 = port
        .trim()
        .parse()
        .map_err(|_| "Pairing port must be a number (e.g. 37001).".to_string())?;
    if port == 0 {
        return Err("Pairing port must be between 1 and 65535.".to_string());
    }

    let code = code.trim().to_string();
    if code.is_empty() {
        return Err("Enter the 6-digit pairing code shown on the phone.".to_string());
    }

    Ok(PairingRequest {
        ip: ip.to_string(),
        port,
        code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_input() {
        let req = validate_pair_input("192.168.1.5", "37001", "483920").unwrap();
        assert_eq!(req.addr(), "192.168.1.5:37001");
        assert_eq!(req.code, "483920");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(validate_pair_input("", "37001", "123456").is_err());
        assert!(validate_pair_input("192.168.1.5", "abc", "123456").is_err());
        assert!(validate_pair_input("192.168.1.5", "0", "123456").is_err());
        assert!(validate_pair_input("192.168.1.5", "37001", "   ").is_err());
        assert!(validate_pair_input("192.168.1.5", "99999", "123456").is_err());
    }

    #[test]
    fn pairing_state_labels() {
        assert_eq!(PairingState::Idle.label(), "Not paired");
        assert!(PairingState::Pairing.in_progress());
        assert!(!PairingState::Paired.in_progress());
    }
}
