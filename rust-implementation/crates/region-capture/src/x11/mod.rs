//! Backend de capture X11 avec support XShm et XFixes

mod shm;
mod cursor;

pub use shm::X11ShmCapture;
pub use cursor::CursorOverlay;

use crate::{CaptureBackend, CaptureError, Capabilities, Frame, Result};
use log::{debug, info, trace};
use region_core::{Rectangle, PixelFormat};
use async_trait::async_trait;
use std::sync::Arc;
use x11rb::protocol::xproto::Screen;
use x11rb::rust_connection::RustConnection;
use x11rb::connection::Connection;

/// Backend de capture X11
pub struct X11Capture {
    connection: Arc<RustConnection>,
    screen: Screen,
    shm_capture: Option<X11ShmCapture>,
    cursor_overlay: Option<CursorOverlay>,
    show_cursor: bool,
}

impl X11Capture {
    /// Crée un nouveau backend X11
    pub fn new() -> Result<Self> {
        debug!("[X11Capture] Connecting to X11 display...");
        let (conn, screen_num) = RustConnection::connect(None)
            .map_err(|e| CaptureError::InitFailed(format!("Connexion X11 échouée: {}", e)))?;
        
        let connection = Arc::new(conn);
        let screen = connection.setup().roots[screen_num].clone();

        info!("[X11Capture] Connected to screen {}: {}x{}", 
            screen_num, screen.width_in_pixels, screen.height_in_pixels);

        Ok(Self {
            connection,
            screen,
            shm_capture: None,
            cursor_overlay: None,
            show_cursor: true,
        })
    }

    /// Vérifie si XShm est disponible
    fn check_xshm_support(&self) -> bool {
        use x11rb::protocol::shm;
        
        match shm::query_version(&*self.connection) {
            Ok(cookie) => cookie.reply().is_ok(),
            Err(_) => false,
        }
    }

    /// Vérifie si XFixes est disponible (pour le curseur)
    fn check_xfixes_support(&self) -> bool {
        use x11rb::protocol::xfixes;
        
        match xfixes::query_version(&*self.connection, 5, 0) {
            Ok(cookie) => cookie.reply().is_ok(),
            Err(_) => false,
        }
    }
}

#[async_trait]
impl CaptureBackend for X11Capture {
    async fn init(&mut self, region: Rectangle) -> Result<()> {
        debug!("[X11Capture] Initializing with region: {:?}", region);
        
        // Vérifier XShm
        if !self.check_xshm_support() {
            return Err(CaptureError::InitFailed(
                "Extension XShm non disponible".to_string()
            ));
        }
        debug!("[X11Capture] XShm extension available");

        // Initialiser la capture XShm
        self.shm_capture = Some(X11ShmCapture::new(
            self.connection.clone(),
            self.screen.root,
            region,
        )?);
        debug!("[X11Capture] XShm capture initialized");

        // Initialiser le curseur si XFixes est disponible
        if self.check_xfixes_support() {
            debug!("[X11Capture] XFixes extension available, initializing cursor overlay");
            self.cursor_overlay = Some(CursorOverlay::new(
                self.connection.clone(),
                self.screen.root,
            )?);
        }

        info!("[X11Capture] Initialization complete");
        Ok(())
    }

    async fn capture_frame(&mut self) -> Result<Frame> {
        let shm = self.shm_capture.as_mut()
            .ok_or_else(|| CaptureError::CaptureFailed("Backend non initialisé".to_string()))?;

        // Capture via XShm
        let mut frame = shm.capture_frame().await?;
        trace!("[X11Capture] Captured frame: {}x{}", frame.width, frame.height);

        // Ajouter le curseur si demandé
        if self.show_cursor {
            if let Some(cursor) = &self.cursor_overlay {
                cursor.apply_to_frame(&mut frame)?;
            }
        }

        Ok(frame)
    }

    async fn capabilities(&self) -> Capabilities {
        Capabilities {
            name: "X11/XShm".to_string(),
            max_fps: 240, // X11 peut être très rapide
            supported_formats: vec![
                PixelFormat::BGRA8888,
                PixelFormat::RGB888,
            ],
            supports_cursor: self.check_xfixes_support(),
            supports_zero_copy: true, // XShm = zero-copy
            supports_region_capture: true,
        }
    }

    async fn set_cursor_visible(&mut self, visible: bool) -> Result<()> {
        self.show_cursor = visible;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        self.shm_capture = None;
        self.cursor_overlay = None;
        Ok(())
    }
    
    async fn get_screen_size(&self) -> Result<(u32, u32)> {
        Ok((
            self.screen.width_in_pixels as u32,
            self.screen.height_in_pixels as u32,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x11_creation() {
        // Ce test peut échouer sur des systèmes sans X11
        match X11Capture::new() {
            Ok(capture) => {
                assert!(capture.screen.width_in_pixels > 0);
                assert!(capture.screen.height_in_pixels > 0);
            }
            Err(e) => {
                // Acceptable si pas de serveur X11
                eprintln!("X11 non disponible (attendu dans certains environnements): {}", e);
            }
        }
    }
}
