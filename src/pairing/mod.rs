//! Wireless Debugging pairing (Phase 3).
//!
//! - [`qr`]: parse the `WIFI:T:ADB;S:…;P:…;;` code + decode QR from pixels.
//! - [`pairing`]: pairing-code request validation + pairing state machine.
//! - [`camera`]: Windows camera access for live QR scanning.
//!
//! Camera frames never leave the machine: decoding is 100% local.

pub mod camera;
pub mod pairing;
pub mod qr;

pub use camera::{CameraFrame, CameraScanner};
pub use pairing::{validate_pair_input, PairingRequest, PairingState};
pub use qr::{decode_qr_from_image_bytes, parse_wireless_qr, QrError, WifiQr};
