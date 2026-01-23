//! Auto backend selector that chooses the best capture method.

use crate::{CaptureBackend, Capabilities, CaptureError, Frame, Result};
use region_core::Rectangle;
use async_trait::async_trait;

#[cfg(feature = "x11")]
use crate::x11::X11Capture;

/// Automatically selects the best capture backend for the current environment.
pub struct AutoBackend {
    inner: Box<dyn CaptureBackend>,
}

impl AutoBackend {
    /// Create a new auto-selecting backend.
    pub fn new() -> Result<Self> {
        // Detect the display server
        let session_type = std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_else(|_| "x11".to_string())
            .to_lowercase();

        // Try Wayland/Portal first if on Wayland
        if session_type.contains("wayland") {
            #[cfg(feature = "portal")]
            {
                return Ok(Self {
                    inner: Box::new(region_portal::PortalBackend::new()),
                });
            }
            #[cfg(not(feature = "portal"))]
            {
                return Err(CaptureError::NotAvailable(
                    "Wayland detected but portal feature not enabled".to_string()
                ));
            }
        }

        // Fall back to X11
        #[cfg(feature = "x11")]
        {
            let x11 = X11Capture::new()?;
            return Ok(Self {
                inner: Box::new(x11),
            });
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
