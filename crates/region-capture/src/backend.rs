//! Capture backend trait and types.

use crate::frame::Frame;
use async_trait::async_trait;
use region_core::{PixelFormat, Rectangle};
use std::fmt;
use thiserror::Error;

/// Main error type for capture operations.
#[derive(Error, Debug)]
pub enum CaptureError {
    /// Backend initialization failed.
    #[error("Backend initialization failed: {0}")]
    InitFailed(String),

    /// Capture operation failed.
    #[error("Capture failed: {0}")]
    CaptureFailed(String),

    /// Unsupported pixel format.
    #[error("Unsupported pixel format: {0}")]
    UnsupportedFormat(String),

    /// Invalid region.
    #[error("Invalid region: {0}")]
    InvalidRegion(String),

    /// Backend not available on this system.
    #[error("Backend not available: {0}")]
    NotAvailable(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error.
    #[error("{0}")]
    Other(String),
}

/// Result type for capture operations.
pub type Result<T> = std::result::Result<T, CaptureError>;

/// Capabilities of a capture backend.
#[derive(Debug, Clone)]
pub struct Capabilities {
    /// Maximum supported frame rate (FPS).
    pub max_fps: u32,

    /// Supported pixel formats.
    pub supported_formats: Vec<PixelFormat>,

    /// Whether cursor capture is supported.
    pub supports_cursor: bool,

    /// Whether zero-copy capture is supported (DMA-BUF).
    pub supports_zero_copy: bool,

    /// Whether the backend can capture a specific region.
    pub supports_region_capture: bool,

    /// Name of the backend.
    pub name: String,
}

impl Capabilities {
    /// Check if a pixel format is supported.
    pub fn supports_format(&self, format: &PixelFormat) -> bool {
        self.supported_formats.contains(format)
    }
}

/// Trait for screen capture backends.
#[async_trait]
pub trait CaptureBackend: Send + Sync {
    /// Initialize the capture backend for a specific region.
    ///
    /// # Arguments
    ///
    /// * `region` - The screen region to capture
    ///
    /// # Errors
    ///
    /// Returns `CaptureError::InitFailed` if initialization fails.
    async fn init(&mut self, region: Rectangle) -> Result<()>;

    /// Capture the next frame.
    ///
    /// This method should block until a new frame is available or return
    /// immediately if buffering is used.
    ///
    /// # Returns
    ///
    /// A `Frame` containing the captured screen data.
    ///
    /// # Errors
    ///
    /// Returns `CaptureError::CaptureFailed` if capture fails.
    async fn capture_frame(&mut self) -> Result<Frame>;

    /// Get the capabilities of this backend.
    async fn capabilities(&self) -> Capabilities;

    /// Set whether to show the cursor in captured frames.
    ///
    /// # Arguments
    ///
    /// * `visible` - Whether the cursor should be visible
    ///
    /// # Note
    ///
    /// This may not be supported by all backends. Check `capabilities().supports_cursor`.
    async fn set_cursor_visible(&mut self, visible: bool) -> Result<()>;

    /// Stop the capture and clean up resources.
    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    /// Get a human-readable name for this backend.
    async fn name(&self) -> String {
        let caps = self.capabilities().await;
        caps.name
    }
    
    /// Get the screen dimensions (width, height).
    ///
    /// This returns the full screen size, useful for taking screenshots
    /// or determining maximum capture region.
    ///
    /// # Returns
    ///
    /// A tuple (width, height) in pixels.
    ///
    /// # Errors
    ///
    /// Returns `CaptureError::NotAvailable` if screen size cannot be determined.
    async fn get_screen_size(&self) -> Result<(u32, u32)>;
    
    /// Capture a full-screen screenshot.
    ///
    /// This is a convenience method that captures the entire screen.
    /// Equivalent to capturing with region (0, 0, screen_width, screen_height).
    ///
    /// # Returns
    ///
    /// A `Frame` containing the full screen capture.
    ///
    /// # Errors
    ///
    /// Returns `CaptureError::CaptureFailed` if capture fails.
    async fn capture_screenshot(&mut self) -> Result<Frame> {
        let (width, height) = self.get_screen_size().await?;
        let region = Rectangle {
            x: 0,
            y: 0,
            width,
            height,
        };
        
        // Temporarily init with full screen region
        self.init(region).await?;
        self.capture_frame().await
    }
}

/// Helper trait for debug formatting of backends.
impl fmt::Debug for dyn CaptureBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CaptureBackend(...)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities() {
        let caps = Capabilities {
            max_fps: 120,
            supported_formats: vec![PixelFormat::RGBA8888, PixelFormat::BGRA8888],
            supports_cursor: true,
            supports_zero_copy: false,
            supports_region_capture: true,
            name: "TestBackend".to_string(),
        };

        assert_eq!(caps.max_fps, 120);
        assert!(caps.supports_format(&PixelFormat::RGBA8888));
        assert!(!caps.supports_format(&PixelFormat::RGB888));
        assert!(caps.supports_cursor);
    }

    #[test]
    fn test_error_display() {
        let err = CaptureError::InitFailed("test error".to_string());
        assert_eq!(format!("{}", err), "Backend initialization failed: test error");

        let err = CaptureError::UnsupportedFormat("RGB888".to_string());
        assert!(format!("{}", err).contains("Unsupported pixel format"));
    }
}
