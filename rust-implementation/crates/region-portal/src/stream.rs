//! Complete Portal backend implementation.

use crate::portal::{PortalCapture, PortalError, RestoreToken};
use crate::pipewire::PipeWireStream;
use region_capture::{CaptureBackend, Capabilities, CaptureError, Frame, Result};
use region_core::{Rectangle, PixelFormat};
use async_trait::async_trait;

/// Portal-based capture backend for Wayland.
pub struct PortalBackend {
    portal: Option<PortalCapture>,
    stream: Option<PipeWireStream>,
    region: Rectangle,
    screen_size: (u32, u32),
    restore_token: Option<RestoreToken>,
}

impl PortalBackend {
    /// Create a new Portal backend.
    pub fn new() -> Self {
        Self {
            portal: None,
            stream: None,
            region: Rectangle::new(0, 0, 1920, 1080),
            screen_size: (1920, 1080),
            restore_token: None,
        }
    }

    /// Create with a restore token for persistent permissions.
    pub fn with_restore_token(token: RestoreToken) -> Self {
        Self {
            portal: None,
            stream: None,
            region: Rectangle::new(0, 0, 1920, 1080),
            screen_size: (1920, 1080),
            restore_token: Some(token),
        }
    }

    /// Get the restore token if available.
    pub fn restore_token(&self) -> Option<&RestoreToken> {
        self.portal.as_ref()?.restore_token()
    }

    /// Check if running on Wayland.
    pub fn is_wayland() -> bool {
        std::env::var("WAYLAND_DISPLAY").is_ok() 
            || std::env::var("XDG_SESSION_TYPE").map(|t| t == "wayland").unwrap_or(false)
    }
}

impl Default for PortalBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CaptureBackend for PortalBackend {
    async fn init(&mut self, region: Rectangle) -> Result<()> {
        self.region = region;
        
        // Initialize portal connection
        let mut portal = PortalCapture::new().await
            .map_err(|e| CaptureError::InitFailed(format!("Portal init failed: {}", e)))?;
        
        // Create session and show permission dialog
        let streams = portal.create_session(self.restore_token.take(), true).await
            .map_err(|e| match e {
                PortalError::UserCancelled => CaptureError::InitFailed("User cancelled".to_string()),
                _ => CaptureError::InitFailed(format!("Session failed: {}", e)),
            })?;
        
        // Get stream info
        let stream_info = streams.first()
            .ok_or_else(|| CaptureError::InitFailed("No streams".to_string()))?;
        
        self.screen_size = (stream_info.width, stream_info.height);
        
        // Get PipeWire fd from portal
        let pipewire_fd = portal.pipewire_fd()
            .ok_or_else(|| CaptureError::InitFailed("No PipeWire fd".to_string()))?;
        
        // Connect to PipeWire with the portal's fd
        let pw_stream = PipeWireStream::connect_with_fd(stream_info.node_id, pipewire_fd).await
            .map_err(|e| CaptureError::InitFailed(format!("PipeWire failed: {}", e)))?;
        
        self.portal = Some(portal);
        self.stream = Some(pw_stream);
        
        Ok(())
    }

    async fn capture_frame(&mut self) -> Result<Frame> {
        let stream = self.stream.as_mut()
            .ok_or_else(|| CaptureError::CaptureFailed("Stream not initialized".to_string()))?;
        
        stream.capture_frame(self.region).await
    }

    async fn capture_screenshot(&mut self) -> Result<Frame> {
        // For portal, we need to capture a full frame first
        let stream = self.stream.as_mut()
            .ok_or_else(|| CaptureError::CaptureFailed("Stream not initialized".to_string()))?;
        
        let full_region = Rectangle::new(0, 0, self.screen_size.0, self.screen_size.1);
        stream.capture_frame(full_region).await
    }

    async fn get_screen_size(&self) -> Result<(u32, u32)> {
        Ok(self.screen_size)
    }

    async fn capabilities(&self) -> Capabilities {
        Capabilities {
            name: "Portal/PipeWire".to_string(),
            max_fps: 60,
            supported_formats: vec![
                PixelFormat::BGRA8888,
                PixelFormat::RGBA8888,
            ],
            supports_cursor: true,
            supports_zero_copy: true, // DMA-BUF support potentially
            supports_region_capture: true, // We handle region extraction
        }
    }

    async fn set_cursor_visible(&mut self, _visible: bool) -> Result<()> {
        // Cursor mode is set at session creation
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(mut stream) = self.stream.take() {
            stream.disconnect().await?;
        }
        if let Some(mut portal) = self.portal.take() {
            let _ = portal.close().await;
        }
        Ok(())
    }
}
