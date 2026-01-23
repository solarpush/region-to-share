//! Portal/PipeWire backend for Wayland screen capture.
//!
//! This crate provides screen capture via the XDG Desktop Portal
//! ScreenCast interface, which is the standard way to capture screens
//! on Wayland compositors.

mod portal;
mod pipewire;
mod stream;

pub use portal::{PortalCapture, PortalError};
pub use pipewire::PipeWireStream;
pub use stream::StreamState;

use region_capture::{CaptureBackend, Capabilities, CaptureError, Frame, Result};
use region_core::{Rectangle, PixelFormat};
use async_trait::async_trait;

/// Portal-based capture backend for Wayland.
pub struct PortalBackend {
    portal: Option<PortalCapture>,
    stream: Option<PipeWireStream>,
    region: Rectangle,
}

impl PortalBackend {
    /// Create a new Portal backend.
    pub fn new() -> Self {
        Self {
            portal: None,
            stream: None,
            region: Rectangle::new(0, 0, 1920, 1080),
        }
    }
}

#[async_trait]
impl CaptureBackend for PortalBackend {
    async fn init(&mut self, region: Rectangle) -> Result<()> {
        self.region = region;
        
        // Initialize portal connection
        let portal = PortalCapture::new().await
            .map_err(|e| CaptureError::InitFailed(format!("Portal init failed: {}", e)))?;
        
        // Create screen cast session
        let session = portal.create_session().await
            .map_err(|e| CaptureError::InitFailed(format!("Session creation failed: {}", e)))?;
        
        // Select sources (request user permission)
        portal.select_sources(&session).await
            .map_err(|e| CaptureError::InitFailed(format!("Source selection failed: {}", e)))?;
        
        // Start the stream
        let stream_info = portal.start_stream(&session).await
            .map_err(|e| CaptureError::InitFailed(format!("Stream start failed: {}", e)))?;
        
        // Connect to PipeWire
        let stream = PipeWireStream::connect(stream_info.node_id).await
            .map_err(|e| CaptureError::InitFailed(format!("PipeWire connection failed: {}", e)))?;
        
        self.portal = Some(portal);
        self.stream = Some(stream);
        
        Ok(())
    }

    async fn capture_frame(&mut self) -> Result<Frame> {
        let stream = self.stream.as_mut()
            .ok_or_else(|| CaptureError::CaptureFailed("Stream not initialized".to_string()))?;
        
        stream.capture_frame(self.region).await
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
            supports_zero_copy: true, // DMA-BUF support
            supports_region_capture: false, // Portal captures full output
        }
    }

    async fn set_cursor_visible(&mut self, visible: bool) -> Result<()> {
        if let Some(portal) = &mut self.portal {
            portal.set_cursor_mode(if visible { 1 } else { 2 }).await
                .map_err(|e| CaptureError::CaptureFailed(format!("Failed to set cursor: {}", e)))?;
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(mut stream) = self.stream.take() {
            stream.disconnect().await?;
        }
        self.portal = None;
        Ok(())
    }
    
    async fn get_screen_size(&self) -> Result<(u32, u32)> {
        // Pour le portal, on ne peut pas facilement obtenir la taille de l'écran
        // avant de démarrer une session. On retourne une valeur par défaut.
        // La vraie taille sera connue après la première capture.
        if let Some(stream) = &self.stream {
            // Si le stream est initialisé, on peut obtenir la taille réelle
            stream.get_stream_size().await
        } else {
            // Fallback: retourner une taille commune
            Ok((1920, 1080))
        }
    }
}

impl Default for PortalBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let backend = PortalBackend::new();
        assert!(backend.portal.is_none());
        assert!(backend.stream.is_none());
    }

    #[tokio::test]
    async fn test_capabilities() {
        let backend = PortalBackend::new();
        let caps = backend.capabilities().await;
        
        assert_eq!(caps.name, "Portal/PipeWire");
        assert!(caps.supports_cursor);
        assert!(caps.supports_zero_copy);
    }
}
