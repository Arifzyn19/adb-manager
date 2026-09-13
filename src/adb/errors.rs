//! Structured ADB error types.
//!
//! Every ADB failure is mapped to a [`AdbError`] variant so the UI can
//! show a friendly message plus expandable technical details.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdbError {
    #[error("ADB executable not found")]
    NotFound {
        searched_paths: Vec<PathBuf>,
        #[source]
        hint: Option<anyhow::Error>,
    },

    #[error("ADB execution failed: {message}")]
    ExecutionFailed {
        message: String,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
    },

    #[error("Device '{serial}' is unauthorized")]
    DeviceUnauthorized { serial: String },

    #[error("Device '{serial}' is offline")]
    DeviceOffline { serial: String },

    #[error("Device '{serial}' not found")]
    DeviceNotFound { serial: String },

    #[error("Pairing failed: {message}")]
    PairingFailed { message: String },

    #[error("Connection failed: {message}")]
    ConnectionFailed { message: String },

    #[error("Permission denied: {message}")]
    PermissionDenied { message: String },

    #[error("File transfer failed: {message}")]
    FileTransferFailed { message: String },

    #[error("Invalid APK: {message}")]
    InvalidApk { message: String },

    #[error("Invalid ADB path: {path}")]
    InvalidPath { path: String },

    #[error("Operation timed out after {seconds}s")]
    Timeout { seconds: u64 },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AdbError {
    /// Short user-friendly title for dialogs / toasts.
    pub fn title(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "ADB not found",
            Self::ExecutionFailed { .. } => "ADB command failed",
            Self::DeviceUnauthorized { .. } => "Device unauthorized",
            Self::DeviceOffline { .. } => "Device offline",
            Self::DeviceNotFound { .. } => "Device not found",
            Self::PairingFailed { .. } => "Pairing failed",
            Self::ConnectionFailed { .. } => "Connection failed",
            Self::PermissionDenied { .. } => "Permission denied",
            Self::FileTransferFailed { .. } => "File transfer failed",
            Self::InvalidApk { .. } => "Invalid APK",
            Self::InvalidPath { .. } => "Invalid ADB path",
            Self::Timeout { .. } => "Operation timed out",
            Self::Io(_) => "I/O error",
            Self::Other(_) => "Unexpected error",
        }
    }

    /// Longer user-friendly guidance.
    pub fn guidance(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => {
                "Android Platform Tools were not found. Install them or browse for adb.exe manually."
            }
            Self::DeviceUnauthorized { .. } => {
                "Unlock your Android device and accept the USB debugging authorization prompt, then retry."
            }
            Self::DeviceOffline { .. } => {
                "The device stopped responding. Reconnect USB / wireless and retry."
            }
            Self::DeviceNotFound { .. } => {
                "The selected device is no longer visible to ADB. Check the connection and refresh."
            }
            Self::PairingFailed { .. } => {
                "Wireless pairing failed. Verify the IP, pairing port and pairing code, then retry."
            }
            Self::ConnectionFailed { .. } => {
                "Could not connect. Verify the IP and ADB port (not the pairing port)."
            }
            Self::PermissionDenied { .. } => {
                "The device refused this operation. It may require a different permission or a rooted device."
            }
            Self::FileTransferFailed { .. } => {
                "A push/pull transfer failed. Check storage space and paths, then retry."
            }
            Self::InvalidApk { .. } => {
                "This file is not a readable APK (not a ZIP, or the manifest cannot be parsed)."
            }
            Self::Timeout { .. } => "The ADB subprocess did not finish in time and was terminated.",
            Self::ExecutionFailed { .. } => "ADB reported an error. See Details for the raw output.",
            Self::InvalidPath { .. } => "The selected file is not a working adb executable.",
            Self::Io(_) => "A local I/O error occurred. See Details.",
            Self::Other(_) => "An unexpected error occurred. See Details.",
        }
    }
}
