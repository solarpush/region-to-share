//! Capture X11 avec XShm (mémoire partagée) pour haute performance

use crate::{CaptureError, Frame, FrameData, Result};
use region_core::{Rectangle, PixelFormat};
use std::sync::Arc;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};
use x11rb::protocol::xproto::Window;
use x11rb::protocol::shm::{self, Seg, ConnectionExt as ShmConnectionExt};
use x11rb::rust_connection::RustConnection;
use x11rb::connection::Connection;
use std::ptr;

/// Segment de mémoire partagée X11
struct ShmSegment {
    seg: Seg,
    #[allow(dead_code)]
    shmid: i32, // Gardé pour le debugging
    addr: *mut u8,
    size: usize,
}

impl ShmSegment {
    /// Crée un nouveau segment XShm
    fn new(connection: &RustConnection, size: usize) -> Result<Self> {
        // Allouer l'ID du segment XShm
        let seg = connection.generate_id()
            .map_err(|e| CaptureError::InitFailed(format!("Génération ID XShm échouée: {}", e)))?;

        // Créer le segment de mémoire partagée POSIX
        let shmid = unsafe {
            libc::shmget(
                libc::IPC_PRIVATE,
                size,
                libc::IPC_CREAT | 0o600, // Lecture/écriture pour le propriétaire
            )
        };
        
        if shmid < 0 {
            return Err(CaptureError::InitFailed(
                format!("shmget échoué: {}", std::io::Error::last_os_error())
            ));
        }

        // Attacher le segment à notre espace d'adressage
        let addr = unsafe { libc::shmat(shmid, ptr::null(), 0) as *mut u8 };
        if addr as isize == -1 {
            unsafe { libc::shmctl(shmid, libc::IPC_RMID, ptr::null_mut()) };
            return Err(CaptureError::InitFailed(
                format!("shmat échoué: {}", std::io::Error::last_os_error())
            ));
        }

        // Attacher le segment au serveur X
        connection.shm_attach(seg, shmid as u32, false)
            .map_err(|e| {
                unsafe {
                    libc::shmdt(addr as *const libc::c_void);
                    libc::shmctl(shmid, libc::IPC_RMID, ptr::null_mut());
                }
                CaptureError::InitFailed(format!("XShm attach échoué: {}", e))
            })?
            .check()
            .map_err(|e| {
                unsafe {
                    libc::shmdt(addr as *const libc::c_void);
                    libc::shmctl(shmid, libc::IPC_RMID, ptr::null_mut());
                }
                CaptureError::InitFailed(format!("XShm attach reply échoué: {}", e))
            })?;

        // Marquer le segment pour suppression quand on n'en aura plus besoin
        // (sera effectivement supprimé quand tous les processus l'auront détaché)
        unsafe { libc::shmctl(shmid, libc::IPC_RMID, ptr::null_mut()) };

        Ok(Self { seg, shmid, addr, size })
    }

    /// Obtient une slice des données
    fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.addr, self.size) }
    }
}

impl Drop for ShmSegment {
    fn drop(&mut self) {
        unsafe {
            libc::shmdt(self.addr as *const libc::c_void);
        }
    }
}

// ShmSegment peut être envoyé entre threads (le pointeur pointe vers de la mémoire partagée)
unsafe impl Send for ShmSegment {}
unsafe impl Sync for ShmSegment {}

/// Capture X11 avec XShm (mémoire partagée zero-copy)
pub struct X11ShmCapture {
    connection: Arc<RustConnection>,
    window: Window,
    region: Rectangle,
    format: PixelFormat,
    sequence_number: u64,
    // Double buffering: deux segments alternés
    segments: [Option<ShmSegment>; 2],
    current_buffer: usize,
    use_shm: bool, // Fallback vers GetImage si XShm échoue
}

impl X11ShmCapture {
    /// Crée une nouvelle capture X11 avec XShm
    pub fn new(
        connection: Arc<RustConnection>,
        window: Window,
        region: Rectangle,
    ) -> Result<Self> {
        let format = Self::detect_pixel_format(&connection)?;
        let buffer_size = (region.width * region.height * 4) as usize;
        
        // Essayer de créer les segments XShm
        let seg0 = ShmSegment::new(&connection, buffer_size).ok();
        let seg1 = ShmSegment::new(&connection, buffer_size).ok();
        
        let use_shm = seg0.is_some() && seg1.is_some();

        Ok(Self {
            connection,
            window,
            region,
            format,
            sequence_number: 0,
            segments: [seg0, seg1],
            current_buffer: 0,
            use_shm,
        })
    }

    /// Capture une frame via XShm (zero-copy) ou GetImage (fallback)
    pub async fn capture_frame(&mut self) -> Result<Frame> {
        self.sequence_number += 1;

        if self.use_shm {
            self.capture_xshm().await
        } else {
            self.capture_getimage().await
        }
    }

    /// Capture via XShm (haute performance, zero-copy pendant la lecture)
    async fn capture_xshm(&mut self) -> Result<Frame> {
        // Utiliser le buffer suivant (double-buffering)
        let buffer_idx = self.current_buffer;
        self.current_buffer = 1 - self.current_buffer;

        let segment = self.segments[buffer_idx].as_ref()
            .ok_or_else(|| CaptureError::CaptureFailed("Segment XShm non initialisé".to_string()))?;

        // Capturer directement dans le segment de mémoire partagée
        shm::get_image(
            &*self.connection,
            self.window,
            self.region.x as i16,
            self.region.y as i16,
            self.region.width as u16,
            self.region.height as u16,
            !0, // Tous les plans
            ImageFormat::Z_PIXMAP.into(),
            segment.seg,
            0, // Offset dans le segment
        )
        .map_err(|e| CaptureError::CaptureFailed(format!("XShm GetImage échoué: {}", e)))?
        .reply()
        .map_err(|e| CaptureError::CaptureFailed(format!("XShm GetImage reply échoué: {}", e)))?;

        // Copier les données du segment (nécessaire car le segment sera réutilisé)
        // Mais on utilise Arc pour éviter les copies lors du partage
        let data_size = (self.region.width * self.region.height * 4) as usize;
        let data = Arc::new(segment.as_slice()[..data_size].to_vec());

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

    /// Fallback vers GetImage (plus lent mais plus stable)
    async fn capture_getimage(&mut self) -> Result<Frame> {
        let reply = self.connection.get_image(
            ImageFormat::Z_PIXMAP,
            self.window,
            self.region.x as i16,
            self.region.y as i16,
            self.region.width as u16,
            self.region.height as u16,
            !0,
        )
        .map_err(|e| CaptureError::CaptureFailed(format!("GetImage échoué: {}", e)))?
        .reply()
        .map_err(|e| CaptureError::CaptureFailed(format!("GetImage reply échoué: {}", e)))?;

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
        
        match screen.root_depth {
            24 | 32 => Ok(PixelFormat::BGRA8888),
            _ => Err(CaptureError::UnsupportedFormat(
                format!("Profondeur de couleur non supportée: {}", screen.root_depth)
            )),
        }
    }
}

impl Drop for X11ShmCapture {
    fn drop(&mut self) {
        // Détacher les segments du serveur X
        for seg in self.segments.iter().flatten() {
            let _ = self.connection.shm_detach(seg.seg);
        }
        let _ = self.connection.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pixel_format_detection() {
        if let Ok((conn, _)) = RustConnection::connect(None) {
            let result = X11ShmCapture::detect_pixel_format(&conn);
            if let Ok(format) = result {
                assert!(format.bytes_per_pixel() >= 3);
            }
        }
    }
}
