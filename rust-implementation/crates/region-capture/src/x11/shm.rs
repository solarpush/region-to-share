//! Capture X11 avec GetImage (version stable sans XShm)

use crate::{CaptureError, Frame, FrameData, Result};
use region_core::{Rectangle, PixelFormat};
use std::sync::Arc;
use x11rb::protocol::xproto::{ImageFormat, ConnectionExt};
use x11rb::protocol::xproto::Window;
use x11rb::rust_connection::RustConnection;
use x11rb::connection::Connection;

/// Capture X11 avec GetImage
pub struct X11ShmCapture {
    connection: Arc<RustConnection>,
    window: Window,
    region: Rectangle,
    format: PixelFormat,
    sequence_number: u64,
}

impl X11ShmCapture {
    /// Crée une nouvelle capture X11
    pub fn new(
        connection: Arc<RustConnection>,
        window: Window,
        region: Rectangle,
    ) -> Result<Self> {
        // Déterminer le format de pixel
        let format = Self::detect_pixel_format(&connection)?;

        Ok(Self {
            connection,
            window,
            region,
            format,
            sequence_number: 0,
        })
    }

    /// Capture une frame via GetImage
    pub async fn capture_frame(&mut self) -> Result<Frame> {
        // Utiliser GetImage (stable, pas de XShm pour éviter les segfaults)
        let reply = self.connection.get_image(
            ImageFormat::Z_PIXMAP.into(),
            self.window,
            self.region.x as i16,
            self.region.y as i16,
            self.region.width as u16,
            self.region.height as u16,
            !0, // Tous les plans
        )
        .map_err(|e| CaptureError::CaptureFailed(format!("GetImage échoué: {}", e)))?
        .reply()
        .map_err(|e| CaptureError::CaptureFailed(format!("GetImage reply échoué: {}", e)))?;

        self.sequence_number += 1;

        // Copier les données
        let data = Arc::new(reply.data);

        Ok(Frame {
            width: self.region.width,
            height: self.region.height,
            format: self.format,
            data: FrameData::Buffer(data),
            timestamp: std::time::Instant::now(),
            sequence: self.sequence_number,
            region: self.region,
        })
    }

    /// Détecte le format de pixel du serveur X11
    fn detect_pixel_format(connection: &RustConnection) -> Result<PixelFormat> {
        let setup = connection.setup();
        let screen = &setup.roots[0];
        
        // Sur la plupart des systèmes X11 modernes, c'est BGRA
        // Vérifier la profondeur et l'ordre des octets
        match screen.root_depth {
            24 | 32 => Ok(PixelFormat::BGRA8888),
            _ => Err(CaptureError::UnsupportedFormat(
                format!("Profondeur de couleur non supportée: {}", screen.root_depth).into()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pixel_format_detection() {
        // Test basique du format de pixel
        if let Ok((conn, _)) = RustConnection::connect(None) {
            let result = X11ShmCapture::detect_pixel_format(&conn);
            // Sur la plupart des systèmes, devrait être BGRA
            if let Ok(format) = result {
                assert!(format.bytes_per_pixel() >= 3);
            }
        }
    }
}
