//! Windows camera access for live QR scanning (nokhwa).
//!
//! Frames are decoded locally by [`crate::pairing::qr`]; nothing is ever
//! uploaded. Dropping the scanner stops the stream and releases the camera.

use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

/// One decoded-ready camera frame (RGBA, row-major).
pub struct CameraFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl CameraFrame {
    /// Number of bytes expected for the RGBA buffer (width × height × 4).
    pub fn expected_len(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }
}

pub struct CameraScanner {
    camera: Camera,
    #[allow(dead_code)]
    index: u32,
}

impl CameraScanner {
    /// Open the first working camera (tries indices 0..3).
    /// Returns a human-readable error when no camera is usable — the UI
    /// shows this plus the manual-pairing fallback.
    pub fn open_default() -> Result<Self, String> {
        let mut last_err = "no camera found".to_string();
        for index in 0..3u32 {
            let requested =
                RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
            match Camera::new(CameraIndex::Index(index), requested) {
                Ok(camera) => return Ok(Self { camera, index }),
                Err(e) => last_err = e.to_string(),
            }
        }
        Err(format!("Could not open a camera ({last_err})."))
    }

    /// Grab one frame. Call at most a few times per second; decode is
    /// throttled by the caller.
    pub fn next_frame(&mut self) -> Result<CameraFrame, String> {
        let buffer = self.camera.frame().map_err(|e| e.to_string())?;
        let img = buffer
            .decode_image::<RgbAFormat>()
            .map_err(|e| e.to_string())?;
        let (width, height) = (img.width(), img.height());
        if width == 0 || height == 0 {
            return Err("Camera returned an empty frame.".to_string());
        }
        Ok(CameraFrame {
            width,
            height,
            rgba: img.into_raw(),
        })
    }
}
