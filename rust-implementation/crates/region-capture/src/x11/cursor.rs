//! Overlay de curseur avec XFixes

use crate::{CaptureError, Frame, Result};
use region_core::Point;
use std::sync::Arc;
use x11rb::protocol::xfixes::ConnectionExt as XFixesConnectionExt;
use x11rb::protocol::xproto::Window;
use x11rb::rust_connection::RustConnection;

/// Informations sur le curseur
#[derive(Debug, Clone)]
pub struct CursorInfo {
    pub position: Point,
    pub hotspot: Point,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Overlay de curseur utilisant XFixes
pub struct CursorOverlay {
    connection: Arc<RustConnection>,
    window: Window,
}

impl CursorOverlay {
    /// Crée un nouveau overlay de curseur
    pub fn new(connection: Arc<RustConnection>, window: Window) -> Result<Self> {
        // Vérifier la version de XFixes
        connection.xfixes_query_version(5, 0)
            .map_err(|e| CaptureError::InitFailed(format!("XFixes query_version échoué: {}", e)))?
            .reply()
            .map_err(|e| CaptureError::InitFailed(format!("XFixes non disponible: {}", e)))?;

        Ok(Self {
            connection,
            window,
        })
    }

    /// Obtient les informations du curseur actuel
    pub fn get_cursor_info(&self) -> Result<Option<CursorInfo>> {
        // Obtenir l'image du curseur
        let cursor_reply = self.connection.xfixes_get_cursor_image()
            .map_err(|e| CaptureError::CaptureFailed(format!("get_cursor_image échoué: {}", e)))?
            .reply()
            .map_err(|e| CaptureError::CaptureFailed(format!("get_cursor_image reply échoué: {}", e)))?;

        if cursor_reply.width == 0 || cursor_reply.height == 0 {
            return Ok(None);
        }

        // Convertir les pixels ARGB32 en RGBA
        let pixel_count = (cursor_reply.width * cursor_reply.height) as usize;
        let mut pixels = Vec::with_capacity(pixel_count * 4);
        
        for &argb in cursor_reply.cursor_image.iter() {
            let a = ((argb >> 24) & 0xFF) as u8;
            let r = ((argb >> 16) & 0xFF) as u8;
            let g = ((argb >> 8) & 0xFF) as u8;
            let b = (argb & 0xFF) as u8;
            
            pixels.push(r);
            pixels.push(g);
            pixels.push(b);
            pixels.push(a);
        }

        Ok(Some(CursorInfo {
            position: Point::new(cursor_reply.x as i32, cursor_reply.y as i32),
            hotspot: Point::new(cursor_reply.xhot as i32, cursor_reply.yhot as i32),
            width: cursor_reply.width as u32,
            height: cursor_reply.height as u32,
            pixels,
        }))
    }

    /// Applique le curseur à une frame
    pub fn apply_to_frame(&self, frame: &mut Frame) -> Result<()> {
        let cursor_info = match self.get_cursor_info()? {
            Some(info) => info,
            None => return Ok(()), // Pas de curseur visible
        };

        // Calculer la position du curseur relative à la région capturée
        let cursor_x = cursor_info.position.x - cursor_info.hotspot.x - frame.region.x;
        let cursor_y = cursor_info.position.y - cursor_info.hotspot.y - frame.region.y;

        // Vérifier si le curseur est dans la région
        if (cursor_x + cursor_info.width as i32) < 0 
            || cursor_x >= frame.region.width as i32
            || (cursor_y + cursor_info.height as i32) < 0 
            || cursor_y >= frame.region.height as i32 {
            return Ok(()); // Curseur hors de la région
        }

        // Obtenir un accès mutable au buffer
        let buffer = match &mut frame.data {
            crate::FrameData::Buffer(arc_buf) => {
                // Cloner le buffer si nécessaire (COW - Copy on Write)
                Arc::make_mut(arc_buf)
            }
            _ => return Err(CaptureError::CaptureFailed(
                "Type de buffer non supporté pour l'overlay".to_string()
            )),
        };

        // Blitter le curseur sur le buffer avec alpha blending
        let frame_bytes_per_pixel = frame.format.bytes_per_pixel();
        
        for cy in 0..cursor_info.height as i32 {
            let frame_y = cursor_y + cy;
            if frame_y < 0 || frame_y >= frame.region.height as i32 {
                continue;
            }

            for cx in 0..cursor_info.width as i32 {
                let frame_x = cursor_x + cx;
                if frame_x < 0 || frame_x >= frame.region.width as i32 {
                    continue;
                }

                // Position dans le buffer de la frame
                let frame_offset = ((frame_y * frame.region.width as i32 + frame_x) 
                    * frame_bytes_per_pixel as i32) as usize;

                // Position dans le buffer du curseur
                let cursor_offset = ((cy * cursor_info.width as i32 + cx) * 4) as usize;

                if cursor_offset + 3 >= cursor_info.pixels.len() {
                    continue;
                }

                let cursor_r = cursor_info.pixels[cursor_offset];
                let cursor_g = cursor_info.pixels[cursor_offset + 1];
                let cursor_b = cursor_info.pixels[cursor_offset + 2];
                let cursor_a = cursor_info.pixels[cursor_offset + 3];

                if cursor_a == 0 {
                    continue; // Pixel transparent
                }

                // Alpha blending
                let alpha = cursor_a as f32 / 255.0;
                let inv_alpha = 1.0 - alpha;

                if frame_offset + 2 < buffer.len() {
                    let frame_b = buffer[frame_offset];
                    let frame_g = buffer[frame_offset + 1];
                    let frame_r = buffer[frame_offset + 2];

                    buffer[frame_offset] = (cursor_b as f32 * alpha + frame_b as f32 * inv_alpha) as u8;
                    buffer[frame_offset + 1] = (cursor_g as f32 * alpha + frame_g as f32 * inv_alpha) as u8;
                    buffer[frame_offset + 2] = (cursor_r as f32 * alpha + frame_r as f32 * inv_alpha) as u8;
                    
                    // Alpha channel si présent
                    if frame_bytes_per_pixel == 4 && frame_offset + 3 < buffer.len() {
                        buffer[frame_offset + 3] = 255; // Opaque après blending
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_info_creation() {
        let info = CursorInfo {
            position: Point::new(100, 200),
            hotspot: Point::new(5, 5),
            width: 32,
            height: 32,
            pixels: vec![0; 32 * 32 * 4],
        };

        assert_eq!(info.position.x, 100);
        assert_eq!(info.position.y, 200);
        assert_eq!(info.width, 32);
        assert_eq!(info.height, 32);
    }
}
