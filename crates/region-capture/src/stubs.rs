//! Cross-platform capture backend scaffolding (Phase 6 targets: macOS & Windows).
//!
//! Keeping these stubs compiled and trait-checked from Phase 1 guarantees that
//! `region-capture` remains cleanly extensible to:
//! - macOS: Apple `ScreenCaptureKit` (`SCStream` + `sourceRect` + system audio)
//! - Windows: `Windows.Graphics.Capture` (`WGC` + DXGI Desktop Duplication)

use crate::{Capabilities, CaptureBackend, CaptureError, Frame, Result};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use async_trait::async_trait;
use region_core::{PixelFormat, Rectangle};

/// macOS `ScreenCaptureKit` backend scaffold (Phase 6).
#[cfg(target_os = "macos")]
pub struct MacOsScreenCaptureKitBackend;

#[cfg(target_os = "macos")]
impl MacOsScreenCaptureKitBackend {
    pub fn new() -> Result<Self> {
        Err(CaptureError::NotAvailable(
            "macOS ScreenCaptureKit backend is planned for Phase 6 (crate `screencapturekit`)"
                .to_string(),
        ))
    }
}

#[cfg(target_os = "macos")]
#[async_trait]
impl CaptureBackend for MacOsScreenCaptureKitBackend {
    async fn init(&mut self, _region: Rectangle) -> Result<()> {
        Err(CaptureError::NotAvailable("Phase 6 target".into()))
    }
    async fn capture_frame(&mut self) -> Result<Frame> {
        Err(CaptureError::NotAvailable("Phase 6 target".into()))
    }
    async fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_fps: 120,
            supported_formats: vec![PixelFormat::BGRA8888],
            supports_cursor: true,
            supports_zero_copy: true,
            supports_region_capture: true,
            name: "macOS/ScreenCaptureKit".into(),
        }
    }
    async fn set_cursor_visible(&mut self, _visible: bool) -> Result<()> {
        Ok(())
    }
    async fn get_screen_size(&self) -> Result<(u32, u32)> {
        Err(CaptureError::NotAvailable("Phase 6 target".into()))
    }
}

/// Windows `Windows.Graphics.Capture` backend scaffold (Phase 6).
#[cfg(target_os = "windows")]
pub struct WindowsGraphicsCaptureBackend;

#[cfg(target_os = "windows")]
impl WindowsGraphicsCaptureBackend {
    pub fn new() -> Result<Self> {
        Err(CaptureError::NotAvailable(
            "Windows.Graphics.Capture backend is planned for Phase 6 (crate `windows-capture`)"
                .to_string(),
        ))
    }
}

#[cfg(target_os = "windows")]
#[async_trait]
impl CaptureBackend for WindowsGraphicsCaptureBackend {
    async fn init(&mut self, _region: Rectangle) -> Result<()> {
        Err(CaptureError::NotAvailable("Phase 6 target".into()))
    }
    async fn capture_frame(&mut self) -> Result<Frame> {
        Err(CaptureError::NotAvailable("Phase 6 target".into()))
    }
    async fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_fps: 120,
            supported_formats: vec![PixelFormat::BGRA8888],
            supports_cursor: true,
            supports_zero_copy: true,
            supports_region_capture: true,
            name: "Windows/WGC".into(),
        }
    }
    async fn set_cursor_visible(&mut self, _visible: bool) -> Result<()> {
        Ok(())
    }
    async fn get_screen_size(&self) -> Result<(u32, u32)> {
        Err(CaptureError::NotAvailable("Phase 6 target".into()))
    }
}

/// Keep unused imports clean on Linux while retaining trait imports for macOS/Windows.
#[allow(dead_code)]
fn _platform_trait_anchor(_: Option<&dyn CaptureBackend>, _: PixelFormat, _: Rectangle) -> Option<(Capabilities, CaptureError, Frame, Result<()>)> {
    None
}
