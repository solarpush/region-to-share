//! Auto backend selector that chooses the best capture method.
//! 
//! Note: For Wayland support, use region_portal::PortalBackend directly
//! from region-ui-egui, as region-portal depends on region-capture
//! (creating a cyclic dependency if we tried to use it here).

use crate::{CaptureBackend, Capabilities, CaptureError, Frame, Result};
use log::{debug, info, warn};
use region_core::Rectangle;
use async_trait::async_trait;

#[cfg(feature = "x11")]
use crate::x11::X11Capture;

/// Automatically selects the best capture backend for the current environment.
/// 
/// Currently only supports X11. For Wayland, use region_portal::PortalBackend.
pub struct AutoBackend {
    inner: Box<dyn CaptureBackend>,
}

impl AutoBackend {
    /// Create a new auto-selecting backend.
    /// 
    /// Returns an error on Wayland - use region_portal::PortalBackend instead.
    pub fn new() -> Result<Self> {
        // Detect the display server
        let session_type = std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_else(|_| "x11".to_string())
            .to_lowercase();

        debug!("[AutoBackend] Detected session type: {}", session_type);

        // On Wayland, caller should use region_portal::PortalBackend directly
        if session_type.contains("wayland") {
            warn!("[AutoBackend] Wayland detected - use region_portal::PortalBackend instead");
            return Err(CaptureError::NotAvailable(
                "Wayland detected - use region_portal::PortalBackend instead".to_string()
            ));
        }

        // X11 backend
        #[cfg(feature = "x11")]
        {
            info!("[AutoBackend] Using X11 capture backend");
            let x11 = X11Capture::new()?;
            Ok(Self {
                inner: Box::new(x11),
            })
        }

        #[cfg(not(feature = "x11"))]
        {
            Err(CaptureError::NotAvailable(
                "No capture backend available".to_string()
            ))
        }
    }

    /// Get the name of the selected backend.
    pub async fn backend_name(&self) -> String {
        self.inner.name().await
    }
}

impl Default for AutoBackend {
    fn default() -> Self {
        Self::new().expect("Failed to create auto backend")
    }
}

#[async_trait]
impl CaptureBackend for AutoBackend {
    async fn init(&mut self, region: Rectangle) -> Result<()> {
        self.inner.init(region).await
    }

    async fn capture_frame(&mut self) -> Result<Frame> {
        self.inner.capture_frame().await
    }

    async fn capabilities(&self) -> Capabilities {
        self.inner.capabilities().await
    }

    async fn set_cursor_visible(&mut self, visible: bool) -> Result<()> {
        self.inner.set_cursor_visible(visible).await
    }

    async fn stop(&mut self) -> Result<()> {
        self.inner.stop().await
    }
    
    async fn get_screen_size(&self) -> Result<(u32, u32)> {
        self.inner.get_screen_size().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_backend_detection() {
        // Test that we can detect and create a backend
        // This may fail on systems without any display server
        match AutoBackend::new() {
            Ok(_backend) => {
                // Success - backend was created
            }
            Err(e) => {
                // Acceptable if no display server available
                eprintln!("No display server available (expected in CI): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_backend_capabilities() {
        if let Ok(backend) = AutoBackend::new() {
            let caps = backend.capabilities().await;
            assert!(!caps.name.is_empty());
            assert!(caps.max_fps > 0);
        }
    }
}
