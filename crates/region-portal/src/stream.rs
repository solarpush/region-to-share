//! Complete Portal backend implementation.

use crate::portal::{PortalCapture, PortalError, RestoreToken};
use crate::pipewire::PipeWireStream;
use region_capture::{CaptureBackend, Capabilities, CaptureError, Frame, Result};
use region_core::{Rectangle, PixelFormat};
use async_trait::async_trait;
use log::{debug, info, trace};

/// Portal-based capture backend for Wayland using PipeWire/DmaBuf.
pub struct PortalBackend {
    portal: Option<PortalCapture>,
    pipewire_stream: Option<PipeWireStream>,
    region: Rectangle,
    screen_size: (u32, u32),
    restore_token: Option<RestoreToken>,
    node_id: Option<u32>,
    sequence: u64,
}

impl PortalBackend {
    /// Create a new Portal backend.
    pub fn new() -> Self {
        Self {
            portal: None,
            pipewire_stream: None,
            region: Rectangle::new(0, 0, 1920, 1080),
            screen_size: (1920, 1080),
            restore_token: None,
            node_id: None,
            sequence: 0,
        }
    }

    /// Create with a restore token for persistent permissions.
    pub fn with_restore_token(token: RestoreToken) -> Self {
        Self {
            portal: None,
            pipewire_stream: None,
            region: Rectangle::new(0, 0, 1920, 1080),
            screen_size: (1920, 1080),
            restore_token: Some(token),
            node_id: None,
            sequence: 0,
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
        debug!("[PortalBackend] Initializing with region: {:?}", region);
        self.region = region;
        
        // Si on a déjà un stream PipeWire actif, réutilise la session existante
        if self.pipewire_stream.is_some() && self.portal.is_some() {
            debug!("[PortalBackend] Reusing existing Portal session");
            return Ok(());
        }
        
        // Initialize portal connection
        debug!("[PortalBackend] Creating portal connection...");
        let mut portal = PortalCapture::new().await
            .map_err(|e| CaptureError::InitFailed(format!("Portal init failed: {}", e)))?;
        
        // Create session and show permission dialog
        debug!("[PortalBackend] Creating session (permission dialog)...");
        let streams = portal.create_session(self.restore_token.take(), true).await
            .map_err(|e| match e {
                PortalError::UserCancelled => CaptureError::InitFailed("User cancelled".to_string()),
                _ => CaptureError::InitFailed(format!("Session failed: {}", e)),
            })?;
        
        // Get stream info
        let stream_info = streams.first()
            .ok_or_else(|| CaptureError::InitFailed("No streams".to_string()))?;
        
        info!("[PortalBackend] Stream: node_id={}, size={}x{}", 
            stream_info.node_id, stream_info.width, stream_info.height);
        
        self.screen_size = (stream_info.width, stream_info.height);
        self.node_id = Some(stream_info.node_id);
        
        // Get PipeWire fd from portal
        let pipewire_fd = portal.pipewire_fd()
            .ok_or_else(|| CaptureError::InitFailed("No PipeWire fd from portal".to_string()))?;
        
        debug!("[PortalBackend] Got PipeWire fd: {}", pipewire_fd);
        
        // Connect to PipeWire stream using portal's fd
        debug!("[PortalBackend] Connecting to PipeWire node {}...", stream_info.node_id);
        let pw_stream = PipeWireStream::connect_with_fd(stream_info.node_id, pipewire_fd).await
            .map_err(|e| CaptureError::InitFailed(format!("PipeWire connect failed: {}", e)))?;
        
        debug!("[PortalBackend] PipeWire connected, waiting for frames...");
        // Give PipeWire time to start receiving frames
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
        self.portal = Some(portal);
        self.pipewire_stream = Some(pw_stream);
        
        info!("[PortalBackend] Initialization complete");
        Ok(())
    }

    async fn capture_frame(&mut self) -> Result<Frame> {
        let pw_stream = self.pipewire_stream.as_mut()
            .ok_or_else(|| CaptureError::CaptureFailed("PipeWire not initialized".to_string()))?;
        
        let frame = pw_stream.capture_frame(self.region).await?;
        self.sequence += 1;
        
        trace!("[PortalBackend] Captured frame #{}: {}x{}", self.sequence, frame.width, frame.height);
        
        Ok(frame)
    }

    async fn capture_screenshot(&mut self) -> Result<Frame> {
        let pw_stream = self.pipewire_stream.as_mut()
            .ok_or_else(|| CaptureError::CaptureFailed("PipeWire not initialized".to_string()))?;
        
        debug!("[PortalBackend] Capturing screenshot...");
        // For screenshot, capture the full screen
        let full_region = Rectangle::new(0, 0, self.screen_size.0, self.screen_size.1);
        let frame = pw_stream.capture_frame(full_region).await?;
        self.sequence += 1;
        
        debug!("[PortalBackend] Screenshot captured: {}x{}", frame.width, frame.height);
        
        Ok(frame)
    }

    async fn get_screen_size(&self) -> Result<(u32, u32)> {
        Ok(self.screen_size)
    }

    async fn capabilities(&self) -> Capabilities {
        Capabilities {
            name: "Portal/PipeWire/DmaBuf".to_string(),
            max_fps: 60,
            supported_formats: vec![
                PixelFormat::BGRA8888,
                PixelFormat::RGBA8888,
            ],
            supports_cursor: true,
            supports_zero_copy: true, // DmaBuf is zero-copy on GPU side
            supports_region_capture: true,
        }
    }

    async fn set_cursor_visible(&mut self, _visible: bool) -> Result<()> {
        // Cursor mode is set at session creation
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(mut pw_stream) = self.pipewire_stream.take() {
            let _ = pw_stream.disconnect().await;
        }
        if let Some(mut portal) = self.portal.take() {
            let _ = portal.close().await;
        }
        Ok(())
    }
}
