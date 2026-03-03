//! DmaBuf import via EGL/OpenGL.
//!
//! This module provides functionality to import DmaBuf file descriptors
//! into OpenGL textures and read them back to CPU memory.

use std::ffi::c_void;
use std::os::fd::RawFd;
use std::ptr;

// EGL constants not in khronos-egl
const EGL_LINUX_DMA_BUF_EXT: u32 = 0x3270;
const EGL_LINUX_DRM_FOURCC_EXT: i32 = 0x3271;
const EGL_DMA_BUF_PLANE0_FD_EXT: i32 = 0x3272;
const EGL_DMA_BUF_PLANE0_OFFSET_EXT: i32 = 0x3273;
const EGL_DMA_BUF_PLANE0_PITCH_EXT: i32 = 0x3274;
// DRM modifier support (EGL_EXT_image_dma_buf_import_modifiers)
const EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT: i32 = 0x3443;
const EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT: i32 = 0x3444;
const EGL_WIDTH: i32 = 0x3057;
const EGL_HEIGHT: i32 = 0x3056;
const EGL_NONE: i32 = 0x3038;

/// Valeur sentinelle : modifier inconnu → on n'ajoute pas les attributs de modifier.
/// Utiliser `DRM_FORMAT_MOD_LINEAR` (0) pour forcer explicitement le mode linéaire.
pub const MODIFIER_UNKNOWN: u64 = u64::MAX;

// DRM format for BGRA (what we typically get from screen capture)
const DRM_FORMAT_ARGB8888: u32 = 0x34325241; // little-endian ARGB
const DRM_FORMAT_XRGB8888: u32 = 0x34325258; // little-endian XRGB (no alpha)

/// EGL context for DmaBuf import operations.
pub struct DmaBufImporter {
    egl: khronos_egl::DynamicInstance<khronos_egl::EGL1_5>,
    display: khronos_egl::Display,
    context: khronos_egl::Context,
    surface: khronos_egl::Surface,  // PBuffer surface
    _config: khronos_egl::Config,
    // OpenGL resources
    texture: u32,
    framebuffer: u32,
    // glEGLImageTargetTexture2DOES function pointer
    gl_egl_image_target_texture_2d: Option<extern "system" fn(u32, *const c_void)>,
    /// true si EGL_EXT_image_dma_buf_import_modifiers est disponible.
    /// Sans cette extension, passer les attribs LO/HI retourne EGL_BAD_PARAMETER.
    supports_modifiers: bool,
}

impl DmaBufImporter {
    /// Create a new DmaBuf importer with an EGL context.
    pub fn new() -> Result<Self, String> {
        unsafe { Self::new_unsafe() }
    }

    unsafe fn new_unsafe() -> Result<Self, String> {
        // Load EGL dynamically
        let egl = khronos_egl::DynamicInstance::<khronos_egl::EGL1_5>::load_required()
            .map_err(|e| format!("Failed to load EGL: {:?}", e))?;

        // Get default display (GBM or surfaceless)
        let display = egl
            .get_display(khronos_egl::DEFAULT_DISPLAY)
            .ok_or("Failed to get EGL display")?;

        // Initialize EGL
        let (_major, _minor) = egl
            .initialize(display)
            .map_err(|e| format!("Failed to initialize EGL: {:?}", e))?;

        // Check for required extensions
        let extensions = egl.query_string(Some(display), khronos_egl::EXTENSIONS)
            .map_err(|e| format!("Failed to query extensions: {:?}", e))?
            .to_string_lossy();
        
        if !extensions.contains("EGL_EXT_image_dma_buf_import") {
            return Err("EGL_EXT_image_dma_buf_import not supported".to_string());
        }
        // L'extension modifiers est optionnelle : sans elle, les attribs LO/HI
        // sont invalides et eglCreateImageKHR retourne EGL_BAD_PARAMETER.
        let supports_modifiers = extensions.contains("EGL_EXT_image_dma_buf_import_modifiers");
        log::debug!("EGL: supports_modifiers={}", supports_modifiers);

        // Choose config for surfaceless context
        let config_attribs = [
            khronos_egl::SURFACE_TYPE, khronos_egl::PBUFFER_BIT,
            khronos_egl::RENDERABLE_TYPE, khronos_egl::OPENGL_ES2_BIT,
            khronos_egl::RED_SIZE, 8,
            khronos_egl::GREEN_SIZE, 8,
            khronos_egl::BLUE_SIZE, 8,
            khronos_egl::ALPHA_SIZE, 8,
            khronos_egl::NONE,
        ];

        let config = egl
            .choose_first_config(display, &config_attribs)
            .map_err(|e| format!("Failed to choose config: {:?}", e))?
            .ok_or("No suitable EGL config found")?;

        // Bind OpenGL ES API
        egl.bind_api(khronos_egl::OPENGL_ES_API)
            .map_err(|e| format!("Failed to bind OpenGL ES API: {:?}", e))?;

        // Create context
        let context_attribs = [
            khronos_egl::CONTEXT_CLIENT_VERSION, 2,
            khronos_egl::NONE,
        ];

        let context = egl
            .create_context(display, config, None, &context_attribs)
            .map_err(|e| format!("Failed to create context: {:?}", e))?;

        // Create a small PBuffer surface (required for make_current on some drivers)
        let pbuffer_attribs = [
            khronos_egl::WIDTH, 1,
            khronos_egl::HEIGHT, 1,
            khronos_egl::NONE,
        ];
        
        let surface = egl
            .create_pbuffer_surface(display, config, &pbuffer_attribs)
            .map_err(|e| format!("Failed to create PBuffer surface: {:?}", e))?;

        // Make context current with the PBuffer surface
        egl.make_current(display, Some(surface), Some(surface), Some(context))
            .map_err(|e| format!("Failed to make context current: {:?}", e))?;

        // Load OpenGL functions
        gl::load_with(|s| {
            egl.get_proc_address(s)
                .map(|p| p as *const c_void)
                .unwrap_or(ptr::null())
        });

        // Get glEGLImageTargetTexture2DOES function
        let gl_egl_image_target_texture_2d = egl
            .get_proc_address("glEGLImageTargetTexture2DOES")
            .map(|p| std::mem::transmute::<_, extern "system" fn(u32, *const c_void)>(p));

        if gl_egl_image_target_texture_2d.is_none() {
            return Err("glEGLImageTargetTexture2DOES not available".to_string());
        }

        // Create texture and framebuffer
        let mut texture = 0u32;
        let mut framebuffer = 0u32;
        gl::GenTextures(1, &mut texture);
        gl::GenFramebuffers(1, &mut framebuffer);

        Ok(Self {
            egl,
            display,
            context,
            surface,
            _config: config,
            texture,
            framebuffer,
            gl_egl_image_target_texture_2d,
            supports_modifiers,
        })
    }

    /// Import a DmaBuf et lit son contenu en mémoire CPU.
    ///
    /// `modifier` est le DRM format modifier négocié par PipeWire (`SPA_FORMAT_VIDEO_modifier`).
    /// Passer [`MODIFIER_UNKNOWN`] si non disponible : les attributs de modifier ne seront
    /// pas inclus dans l'appel EGL (comportement legacy, peut échouer avec certains drivers).
    pub fn import_dmabuf(
        &self,
        fd: RawFd,
        width: u32,
        height: u32,
        stride: u32,
        offset: u32,
        fourcc: u32,
        modifier: u64,
    ) -> Result<Vec<u8>, String> {
        unsafe { self.import_dmabuf_unsafe(fd, width, height, stride, offset, fourcc, modifier) }
    }

    unsafe fn import_dmabuf_unsafe(
        &self,
        fd: RawFd,
        width: u32,
        height: u32,
        stride: u32,
        offset: u32,
        fourcc: u32,
        modifier: u64,
    ) -> Result<Vec<u8>, String> {
        // Make sure our context is current
        self.egl
            .make_current(self.display, Some(self.surface), Some(self.surface), Some(self.context))
            .map_err(|e| format!("Failed to make context current: {:?}", e))?;

        // Construire les attributs EGLImage dynamiquement selon la présence d'un modifier.
        // Sans modifier correct, eglCreateImageKHR peut retourner GL_INVALID_OPERATION
        // quand le driver du sandbox ≠ driver hôte (sand-boxing Flatpak, etc.).
        let mut attribs: Vec<i32> = vec![
            EGL_WIDTH, width as i32,
            EGL_HEIGHT, height as i32,
            EGL_LINUX_DRM_FOURCC_EXT, fourcc as i32,
            EGL_DMA_BUF_PLANE0_FD_EXT, fd,
            EGL_DMA_BUF_PLANE0_OFFSET_EXT, offset as i32,
            EGL_DMA_BUF_PLANE0_PITCH_EXT, stride as i32,
        ];
        if self.supports_modifiers && modifier != MODIFIER_UNKNOWN {
            // Passer le modifier DRM 64 bits en deux i32 (lo/hi).
            // Uniquement si EGL_EXT_image_dma_buf_import_modifiers est présent,
            // sinon eglCreateImageKHR retourne EGL_BAD_PARAMETER.
            attribs.extend_from_slice(&[
                EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT, (modifier & 0xffff_ffff) as i32,
                EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT, (modifier >> 32) as i32,
            ]);
        }
        attribs.push(EGL_NONE);

        // Get eglCreateImageKHR
        let create_image: extern "system" fn(
            khronos_egl::EGLDisplay,
            khronos_egl::EGLContext,
            u32,
            *const c_void,
            *const i32,
        ) -> *const c_void = std::mem::transmute(
            self.egl
                .get_proc_address("eglCreateImageKHR")
                .ok_or("eglCreateImageKHR not available")?,
        );

        let destroy_image: extern "system" fn(
            khronos_egl::EGLDisplay,
            *const c_void,
        ) -> khronos_egl::Boolean = std::mem::transmute(
            self.egl
                .get_proc_address("eglDestroyImageKHR")
                .ok_or("eglDestroyImageKHR not available")?,
        );

        let mut image = create_image(
            self.display.as_ptr(),
            khronos_egl::NO_CONTEXT,
            EGL_LINUX_DMA_BUF_EXT,
            ptr::null(),
            attribs.as_ptr(),
        );

        // Si l'appel échoue avec modifier (BadParameter = le modifier vendor n'est pas
        // supporté par ce Mesa), on réessaie sans les attribs modifier (mode legacy).
        // C'est le cas typique en Flatpak : l'extension est déclarée mais
        // le modifier AMD/Intel propriétaire n'est pas dans la liste supportée.
        if image.is_null() {
            let err = self.egl.get_error();
            // EGL_BAD_PARAMETER = 0x300C
            let had_modifier = attribs.windows(2).any(|w| w[0] == EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT);
            if had_modifier {
                log::debug!("EGL: eglCreateImageKHR avec modifier échoué ({:?}), retry sans modifier", err);
                // Reconstruire attribs sans les entrées modifier
                let base_attribs: Vec<i32> = vec![
                    EGL_WIDTH, width as i32,
                    EGL_HEIGHT, height as i32,
                    EGL_LINUX_DRM_FOURCC_EXT, fourcc as i32,
                    EGL_DMA_BUF_PLANE0_FD_EXT, fd,
                    EGL_DMA_BUF_PLANE0_OFFSET_EXT, offset as i32,
                    EGL_DMA_BUF_PLANE0_PITCH_EXT, stride as i32,
                    EGL_NONE,
                ];
                image = create_image(
                    self.display.as_ptr(),
                    khronos_egl::NO_CONTEXT,
                    EGL_LINUX_DMA_BUF_EXT,
                    ptr::null(),
                    base_attribs.as_ptr(),
                );
            }
            if image.is_null() {
                let err2 = self.egl.get_error();
                return Err(format!("Failed to create EGLImage: {:?}", err2));
            }
        }

        // Bind texture and attach EGLImage
        gl::BindTexture(gl::TEXTURE_2D, self.texture);
        
        let gl_egl_image_target = self.gl_egl_image_target_texture_2d
            .ok_or("glEGLImageTargetTexture2DOES not available")?;
        gl_egl_image_target(gl::TEXTURE_2D, image);

        // Check for GL errors
        let gl_error = gl::GetError();
        if gl_error != gl::NO_ERROR {
            destroy_image(self.display.as_ptr(), image);
            return Err(format!("GL error after image target: 0x{:x}", gl_error));
        }

        // Setup framebuffer
        gl::BindFramebuffer(gl::FRAMEBUFFER, self.framebuffer);
        gl::FramebufferTexture2D(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            self.texture,
            0,
        );

        // Check framebuffer status
        let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
        if status != gl::FRAMEBUFFER_COMPLETE {
            destroy_image(self.display.as_ptr(), image);
            return Err(format!("Framebuffer incomplete: 0x{:x}", status));
        }

        // Read pixels
        let size = (width * height * 4) as usize;
        let mut pixels = vec![0u8; size];
        
        gl::ReadPixels(
            0,
            0,
            width as i32,
            height as i32,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            pixels.as_mut_ptr() as *mut c_void,
        );

        // Check for GL errors
        let gl_error = gl::GetError();
        if gl_error != gl::NO_ERROR {
            destroy_image(self.display.as_ptr(), image);
            return Err(format!("GL error after ReadPixels: 0x{:x}", gl_error));
        }

        // Cleanup
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        destroy_image(self.display.as_ptr(), image);

        Ok(pixels)
    }

    /// Get the FOURCC code for the given format string or guess based on common formats.
    pub fn guess_fourcc(format_hint: Option<&str>) -> u32 {
        match format_hint {
            Some("ARGB8888") | Some("BGRA") => DRM_FORMAT_ARGB8888,
            Some("XRGB8888") | Some("BGRX") | Some("BGRx") => DRM_FORMAT_XRGB8888,
            _ => DRM_FORMAT_XRGB8888, // Default for screen capture
        }
    }
}

impl Drop for DmaBufImporter {
    fn drop(&mut self) {
        unsafe {
            if self.texture != 0 {
                gl::DeleteTextures(1, &self.texture);
            }
            if self.framebuffer != 0 {
                gl::DeleteFramebuffers(1, &self.framebuffer);
            }
            let _ = self.egl.destroy_surface(self.display, self.surface);
            let _ = self.egl.destroy_context(self.display, self.context);
            let _ = self.egl.terminate(self.display);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_importer_creation() {
        // This test will only pass on systems with proper EGL support
        match DmaBufImporter::new() {
            Ok(importer) => {
                println!("DmaBufImporter created successfully");
                drop(importer);
            }
            Err(e) => {
                println!("DmaBufImporter creation failed (expected on CI): {}", e);
            }
        }
    }
}
