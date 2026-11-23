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

pub type Result<T> = std::result::Result<T, CaptureError>;

/// Capabilities of a capture backend.
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub max_fps: u32,
    pub supported_formats: Vec<PixelFormat>,
    pub supports_cursor: bool,
    pub supports_zero_copy: bool,
    pub supports_region_capture: bool,
    pub name: String,
}

impl Capabilities {
    pub fn supports_format(&self, format: &PixelFormat) -> bool {
        self.supported_formats.contains(format)
    }
}

/// Unified async trait implemented by all screen capture backends
/// (Wayland Portal/PipeWire, X11 XShm, macOS ScreenCaptureKit, Windows WGC).
#[async_trait]
pub trait CaptureBackend: Send + Sync {
    /// Initialize (or re-target) the capture backend for a specific region.
    async fn init(&mut self, region: Rectangle) -> Result<()>;

    /// Dynamically update the captured region without tearing down the session
    /// (used for live re-cropping and `region-to-share set-region`).
    async fn update_region(&mut self, region: Rectangle) -> Result<()> {
        self.init(region).await
    }

    /// Capture the next frame.
    async fn capture_frame(&mut self) -> Result<Frame>;

    /// Query backend capabilities.
    async fn capabilities(&self) -> Capabilities;

    /// Toggle cursor visibility when supported dynamically.
    async fn set_cursor_visible(&mut self, visible: bool) -> Result<()>;

    /// Stop capture and release OS/compositor resources.
    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    /// Human-readable backend name.
    async fn name(&self) -> String {
        self.capabilities().await.name
    }

    /// Logical screen size `(width, height)` in the coordinate space expected by `init`.
    async fn get_screen_size(&self) -> Result<(u32, u32)>;

    /// Returns `true` if the underlying stream is *already* cropped to the user's
    /// chosen region or window by the compositor (e.g. KDE Plasma 6 `stream_region`),
    /// meaning no secondary overlay selection should be forced.
    fn is_precropped_stream(&self) -> bool {
        false
    }

    /// Returns a newly issued persistent permission token (e.g. Wayland ScreenCast
    /// `restore_token`) so the caller can save it in `region-config`.
    fn restore_token_str(&self) -> Option<String> {
        None
    }

    /// Capture a single full-source screenshot frame.
    async fn capture_screenshot(&mut self) -> Result<Frame> {
        let (width, height) = self.get_screen_size().await?;
        let region = Rectangle::new(0, 0, width, height);
        self.init(region).await?;
        self.capture_frame().await
    }
}

impl fmt::Debug for dyn CaptureBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CaptureBackend(...)")
    }
}
