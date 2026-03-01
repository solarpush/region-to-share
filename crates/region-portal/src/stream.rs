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
    /// Portal-reported stream size, which may differ from actual PipeWire
    /// frame dimensions under Wayland fractional scaling.
    portal_reported_size: Option<(u32, u32)>,
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
            portal_reported_size: None,
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
            portal_reported_size: None,
            restore_token: Some(token),
            node_id: None,
            sequence: 0,
        }
    }

    /// Get the restore token if available.
    pub fn restore_token(&self) -> Option<&RestoreToken> {
        self.portal.as_ref()?.restore_token()
    }

    /// Scale a region from portal-reported coordinate space to actual stream
    /// pixel space. Under Wayland fractional scaling, the portal may report a
    /// logical size (e.g. 3840x1620) while the PipeWire stream delivers frames
    /// at the native resolution (e.g. 5120x2160).
    fn scale_region_to_stream(&self, region: Rectangle, stream_size: (u32, u32)) -> Rectangle {
        let (stream_w, stream_h) = stream_size;
        if let Some((portal_w, portal_h)) = self.portal_reported_size {
            if stream_w > 0 && stream_h > 0
                && (stream_w != portal_w || stream_h != portal_h)
            {
                let scale_x = stream_w as f64 / portal_w as f64;
                let scale_y = stream_h as f64 / portal_h as f64;
                debug!(
                    "[PortalBackend] Scaling region {:?}: portal={}x{}, stream={}x{}, \
                     scale={:.4}x{:.4}",
                    region, portal_w, portal_h, stream_w, stream_h, scale_x, scale_y
                );
                return Rectangle::new(
                    (region.x as f64 * scale_x) as i32,
                    (region.y as f64 * scale_y) as i32,
                    (region.width as f64 * scale_x) as u32,
                    (region.height as f64 * scale_y) as u32,
                );
            }
        }
        region
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
        self.portal_reported_size = Some((stream_info.width, stream_info.height));
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

        // Update screen_size from actual PipeWire stream dimensions, which may
        // differ from portal-reported size under Wayland fractional scaling.
        let (actual_w, actual_h) = pw_stream.stream_size();
        if actual_w > 0 && actual_h > 0
            && (actual_w != self.screen_size.0 || actual_h != self.screen_size.1)
        {
            info!(
                "[PortalBackend] Fractional scaling detected: portal reported {}x{}, \
                 stream actual {}x{}",
                self.screen_size.0, self.screen_size.1, actual_w, actual_h
            );
            self.screen_size = (actual_w, actual_h);
        }

        self.portal = Some(portal);
        self.pipewire_stream = Some(pw_stream);

        info!("[PortalBackend] Initialization complete");
        Ok(())
    }

    async fn capture_frame(&mut self) -> Result<Frame> {
        let stream_size = self.pipewire_stream.as_ref()
            .map(|s| s.stream_size())
            .unwrap_or((0, 0));
        let region = self.scale_region_to_stream(self.region, stream_size);

        let pw_stream = self.pipewire_stream.as_mut()
            .ok_or_else(|| CaptureError::CaptureFailed("PipeWire not initialized".to_string()))?;
        let frame = pw_stream.capture_frame(region).await?;
        self.sequence += 1;

        trace!("[PortalBackend] Captured frame #{}: {}x{}", self.sequence, frame.width, frame.height);

        Ok(frame)
    }

    async fn capture_screenshot(&mut self) -> Result<Frame> {
        let pw_stream = self.pipewire_stream.as_mut()
            .ok_or_else(|| CaptureError::CaptureFailed("PipeWire not initialized".to_string()))?;

        debug!("[PortalBackend] Capturing screenshot...");

        // Use actual stream dimensions for full-screen capture, since the
        // portal-reported size may differ under fractional scaling.
        let (stream_w, stream_h) = pw_stream.stream_size();
        let (w, h) = if stream_w > 0 && stream_h > 0 {
            (stream_w, stream_h)
        } else {
            self.screen_size
        };
        let full_region = Rectangle::new(0, 0, w, h);
        let frame = pw_stream.capture_frame(full_region).await?;
        self.sequence += 1;

        // Update screen_size if it changed (first frame may arrive late)
        if frame.width != self.screen_size.0 || frame.height != self.screen_size.1 {
            debug!(
                "[PortalBackend] Updating screen_size from {}x{} to {}x{}",
                self.screen_size.0, self.screen_size.1, frame.width, frame.height
            );
            self.screen_size = (frame.width, frame.height);
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a PortalBackend with specific portal-reported size.
    fn backend_with_portal_size(portal_w: u32, portal_h: u32) -> PortalBackend {
        let mut backend = PortalBackend::new();
        backend.portal_reported_size = Some((portal_w, portal_h));
        backend
    }

    #[test]
    fn scale_region_fractional_scaling_133_percent() {
        // 133% scaling: portal reports 3840x1620, stream delivers 5120x2160
        let backend = backend_with_portal_size(3840, 1620);
        let region = Rectangle::new(100, 200, 800, 600);
        let scaled = backend.scale_region_to_stream(region, (5120, 2160));

        // scale = 5120/3840 = 1.3333, 2160/1620 = 1.3333
        assert_eq!(scaled.x, 133);   // 100 * 1.3333
        assert_eq!(scaled.y, 266);   // 200 * 1.3333
        assert_eq!(scaled.width, 1066);  // 800 * 1.3333
        assert_eq!(scaled.height, 800);  // 600 * 1.3333
    }

    #[test]
    fn scale_region_fractional_scaling_150_percent() {
        // 150% scaling: portal reports 1280x720, stream delivers 1920x1080
        let backend = backend_with_portal_size(1280, 720);
        let region = Rectangle::new(50, 100, 640, 360);
        let scaled = backend.scale_region_to_stream(region, (1920, 1080));

        // scale = 1920/1280 = 1.5, 1080/720 = 1.5
        assert_eq!(scaled.x, 75);
        assert_eq!(scaled.y, 150);
        assert_eq!(scaled.width, 960);
        assert_eq!(scaled.height, 540);
    }

    #[test]
    fn scale_region_no_scaling_needed() {
        // No fractional scaling: portal reports same as stream
        let backend = backend_with_portal_size(1920, 1080);
        let region = Rectangle::new(100, 200, 800, 600);
        let scaled = backend.scale_region_to_stream(region, (1920, 1080));

        // No change expected
        assert_eq!(scaled, region);
    }

    #[test]
    fn scale_region_no_portal_size() {
        // No portal size stored (e.g., backend not yet initialized)
        let backend = PortalBackend::new();
        let region = Rectangle::new(100, 200, 800, 600);
        let scaled = backend.scale_region_to_stream(region, (5120, 2160));

        // No change expected when portal_reported_size is None
        assert_eq!(scaled, region);
    }

    #[test]
    fn scale_region_zero_stream_size() {
        // Stream size not yet known (0x0)
        let backend = backend_with_portal_size(3840, 1620);
        let region = Rectangle::new(100, 200, 800, 600);
        let scaled = backend.scale_region_to_stream(region, (0, 0));

        // No change expected when stream size is zero
        assert_eq!(scaled, region);
    }

    #[test]
    fn scale_region_origin() {
        // Region at origin should stay at origin
        let backend = backend_with_portal_size(3840, 1620);
        let region = Rectangle::new(0, 0, 3840, 1620);
        let scaled = backend.scale_region_to_stream(region, (5120, 2160));

        assert_eq!(scaled.x, 0);
        assert_eq!(scaled.y, 0);
        assert_eq!(scaled.width, 5120);
        assert_eq!(scaled.height, 2160);
    }
}
