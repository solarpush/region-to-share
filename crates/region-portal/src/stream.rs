//! Complete Portal backend implementation with RestoreToken persistence,
//! native KDE Plasma 6 direct-region mode, fractional scaling awareness,
//! and in-callback PipeWire cropping.

use crate::pipewire::PipeWireStream;
use crate::portal::{PortalCapture, PortalError, PortalSessionMode, RestoreToken};
use async_trait::async_trait;
use log::{debug, info, trace, warn};
use region_capture::{Capabilities, CaptureBackend, CaptureError, Frame, Result};
use region_core::{PixelFormat, Rectangle};
use std::os::fd::AsRawFd;

/// Options controlling how [`PortalBackend`] creates its Wayland screencast session.
#[derive(Debug, Clone)]
pub struct PortalBackendOptions {
    pub restore_token: Option<RestoreToken>,
    pub include_cursor: bool,
    pub session_mode: PortalSessionMode,
    pub prefer_dmabuf: bool,
}

impl Default for PortalBackendOptions {
    fn default() -> Self {
        Self {
            restore_token: None,
            include_cursor: true,
            session_mode: PortalSessionMode::MonitorForOverlay,
            prefer_dmabuf: true,
        }
    }
}

/// Portal-based capture backend for Wayland using PipeWire.
pub struct PortalBackend {
    portal: Option<PortalCapture>,
    pipewire_stream: Option<PipeWireStream>,
    region: Rectangle,
    screen_size: (u32, u32),
    portal_reported_size: Option<(u32, u32)>,
    options: PortalBackendOptions,
    node_id: Option<u32>,
    is_precropped: bool,
    sequence: u64,
}

impl PortalBackend {
    pub fn new() -> Self {
        Self::with_options(PortalBackendOptions::default())
    }

    pub fn with_options(options: PortalBackendOptions) -> Self {
        Self {
            portal: None,
            pipewire_stream: None,
            region: Rectangle::new(0, 0, 1920, 1080),
            screen_size: (1920, 1080),
            portal_reported_size: None,
            options,
            node_id: None,
            is_precropped: false,
            sequence: 0,
        }
    }

    pub fn with_restore_token(token: RestoreToken) -> Self {
        Self::with_options(PortalBackendOptions {
            restore_token: Some(token),
            ..Default::default()
        })
    }

    /// Returns the latest restore token issued by the portal (for persistence in `region-config`).
    pub fn restore_token(&self) -> Option<&RestoreToken> {
        self.portal
            .as_ref()
            .and_then(|p| p.restore_token())
            .or(self.options.restore_token.as_ref())
    }

    /// Convert a region from portal logical space to native PipeWire pixels.
    fn scale_region_to_stream(&self, region: Rectangle, stream_size: (u32, u32)) -> Rectangle {
        let (stream_w, stream_h) = stream_size;
        if let Some((portal_w, portal_h)) = self.portal_reported_size {
            if stream_w > 0 && stream_h > 0 && (stream_w != portal_w || stream_h != portal_h) {
                let scale_x = stream_w as f64 / portal_w as f64;
                let scale_y = stream_h as f64 / portal_h as f64;
                debug!(
                    "[PortalBackend] Scaling region {:?}: portal={}x{}, stream={}x{}, scale={:.4}x{:.4}",
                    region, portal_w, portal_h, stream_w, stream_h, scale_x, scale_y
                );
                return Rectangle::new(
                    (region.x as f64 * scale_x).round() as i32,
                    (region.y as f64 * scale_y).round() as i32,
                    ((region.width as f64 * scale_x).round() as u32).max(1),
                    ((region.height as f64 * scale_y).round() as u32).max(1),
                );
            }
        }
        region
    }

    fn sync_callback_crop(&self) {
        if let Some(pw) = &self.pipewire_stream {
            if self.is_precropped {
                pw.set_active_crop(None);
            } else {
                let stream_size = pw.stream_size();
                if stream_size.0 > 0 && stream_size.1 > 0 {
                    let scaled = self.scale_region_to_stream(self.region, stream_size);
                    pw.set_active_crop(Some(scaled));
                }
            }
        }
    }

    pub fn is_wayland() -> bool {
        region_platform::DisplayServer::detect().is_wayland()
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
        debug!("[PortalBackend] init region={:?} mode={:?}", region, self.options.session_mode);
        self.region = region;

        // Reuse existing live PipeWire session without re-prompting the portal
        if self.pipewire_stream.is_some() && self.portal.is_some() {
            debug!("[PortalBackend] Reusing existing Portal/PipeWire session");
            self.sync_callback_crop();
            if let Some(pw) = self.pipewire_stream.as_mut() {
                pw.clear_cached_and_drain();
            }
            return Ok(());
        }

        let mut portal = PortalCapture::new()
            .await
            .map_err(|e| CaptureError::InitFailed(format!("Portal init failed: {}", e)))?;

        let streams = portal
            .create_session(
                self.options.restore_token.take(),
                self.options.include_cursor,
                self.options.session_mode,
            )
            .await
            .map_err(|e| match e {
                PortalError::UserCancelled => {
                    CaptureError::InitFailed("User cancelled".to_string())
                }
                _ => CaptureError::InitFailed(format!("Session failed: {}", e)),
            })?;

        let stream_info = streams
            .first()
            .ok_or_else(|| CaptureError::InitFailed("No streams".to_string()))?;

        info!(
            "[PortalBackend] Stream node_id={} size={}x{} type={} precropped={}",
            stream_info.node_id,
            stream_info.width,
            stream_info.height,
            stream_info.source_type,
            stream_info.is_precropped
        );

        self.screen_size = (stream_info.width, stream_info.height);
        self.portal_reported_size = Some((stream_info.width, stream_info.height));
        self.node_id = Some(stream_info.node_id);
        self.is_precropped = stream_info.is_precropped;

        let pipewire_owned_fd = portal
            .take_pipewire_fd()
            .ok_or_else(|| CaptureError::InitFailed("No PipeWire fd from portal".to_string()))?;
        let pipewire_fd = pipewire_owned_fd.as_raw_fd();
        std::mem::forget(pipewire_owned_fd);

        let mut pw_stream = PipeWireStream::connect_with_fd(
            stream_info.node_id,
            pipewire_fd,
            self.options.prefer_dmabuf,
        )
        .await
        .map_err(|e| CaptureError::InitFailed(format!("PipeWire connect failed: {}", e)))?;

        tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;

        let (actual_w, actual_h) = pw_stream.probe_stream_size().await;
        if actual_w > 0 && actual_h > 0 {
            if actual_w != self.screen_size.0 || actual_h != self.screen_size.1 {
                warn!(
                    "[PortalBackend] Fractional scaling detected: portal={}x{}, PipeWire={}x{}",
                    self.screen_size.0, self.screen_size.1, actual_w, actual_h
                );
                self.screen_size = (actual_w, actual_h);
            }
            if self.is_precropped {
                // In KDE NativeDirect mode, the stream IS the selected region!
                self.region = Rectangle::new(0, 0, actual_w, actual_h);
            }
        }

        self.portal = Some(portal);
        self.pipewire_stream = Some(pw_stream);
        self.sync_callback_crop();

        info!("[PortalBackend] Initialization complete");
        Ok(())
    }

    async fn update_region(&mut self, region: Rectangle) -> Result<()> {
        self.region = region;
        self.sync_callback_crop();
        if let Some(pw) = self.pipewire_stream.as_mut() {
            pw.clear_cached_and_drain();
        }
        Ok(())
    }

    fn is_precropped_stream(&self) -> bool {
        self.is_precropped
    }

    fn restore_token_str(&self) -> Option<String> {
        self.restore_token().map(|t| t.0.clone())
    }

    async fn capture_frame(&mut self) -> Result<Frame> {
        let stream_size = self
            .pipewire_stream
            .as_ref()
            .map(|s| s.stream_size())
            .unwrap_or((0, 0));

        let target_region = if self.is_precropped && stream_size.0 > 0 && stream_size.1 > 0 {
            Rectangle::new(0, 0, stream_size.0, stream_size.1)
        } else {
            self.scale_region_to_stream(self.region, stream_size)
        };

        let pw_stream = self
            .pipewire_stream
            .as_mut()
            .ok_or_else(|| CaptureError::CaptureFailed("PipeWire not initialized".to_string()))?;

        let frame = pw_stream.capture_frame(target_region).await?;
        self.sequence += 1;
        trace!(
            "[PortalBackend] Frame #{}: {}x{}",
            self.sequence,
            frame.width,
            frame.height
        );
        Ok(frame)
    }

    async fn capture_screenshot(&mut self) -> Result<Frame> {
        let pw_stream = self
            .pipewire_stream
            .as_mut()
            .ok_or_else(|| CaptureError::CaptureFailed("PipeWire not initialized".to_string()))?;

        // Temporarily disable in-callback crop to capture the full desktop frame
        pw_stream.set_active_crop(None);
        pw_stream.drop_cropped_cached_and_drain();

        let (stream_w, stream_h) = pw_stream.stream_size();
        let (w, h) = if stream_w > 0 && stream_h > 0 {
            (stream_w, stream_h)
        } else {
            self.screen_size
        };
        let full_region = Rectangle::new(0, 0, w, h);
        let frame = pw_stream
            .capture_frame_with_timeout(full_region, std::time::Duration::from_millis(2500))
            .await?;
        self.sequence += 1;

        if frame.width != self.screen_size.0 || frame.height != self.screen_size.1 {
            self.screen_size = (frame.width, frame.height);
        }

        Ok(frame)
    }

    async fn get_screen_size(&self) -> Result<(u32, u32)> {
        Ok(self.portal_reported_size.unwrap_or(self.screen_size))
    }

    async fn capabilities(&self) -> Capabilities {
        Capabilities {
            name: if self.is_precropped {
                "Portal/PipeWire (KDE Native Region)".to_string()
            } else {
                "Portal/PipeWire".to_string()
            },
            max_fps: 60,
            supported_formats: vec![PixelFormat::BGRA8888, PixelFormat::RGBA8888],
            supports_cursor: true,
            supports_zero_copy: true,
            supports_region_capture: true,
        }
    }

    async fn set_cursor_visible(&mut self, _visible: bool) -> Result<()> {
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

    fn backend_with_portal_size(portal_w: u32, portal_h: u32) -> PortalBackend {
        let mut backend = PortalBackend::new();
        backend.portal_reported_size = Some((portal_w, portal_h));
        backend
    }

    #[test]
    fn scale_region_fractional_scaling_ultrawide() {
        let backend = backend_with_portal_size(3840, 1620);
        let region = Rectangle::new(100, 200, 800, 600);
        let scaled = backend.scale_region_to_stream(region, (5120, 2160));
        assert_eq!(scaled.x, 133);
        assert_eq!(scaled.y, 267);
        assert_eq!(scaled.width, 1067);
        assert_eq!(scaled.height, 800);
    }

    #[test]
    fn scale_region_no_scaling_needed() {
        let backend = backend_with_portal_size(1920, 1080);
        let region = Rectangle::new(100, 200, 800, 600);
        assert_eq!(backend.scale_region_to_stream(region, (1920, 1080)), region);
    }
}
