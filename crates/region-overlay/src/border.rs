//! Live click-through capture border indicator around the active screen region.
//!
//! Draws a crisp, non-interactive 3px frame outside the captured area so the user
//! always knows what portion of the screen is being shared (emerald) or recorded
//! (coral red). Mouse clicks pass straight through the border.

use log::debug;
use region_core::Rectangle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// Visual color style for the capture border.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    /// Emerald green — live region sharing.
    Sharing,
    /// Coral red — active disk recording.
    Recording,
    /// Amber — paused.
    Paused,
}

impl BorderStyle {
    /// 24-bit RGB color (`0xRRGGBB`).
    pub const fn rgb_u32(self) -> u32 {
        match self {
            Self::Sharing => 0x2DD4BF,
            Self::Recording => 0xFF4D5E,
            Self::Paused => 0xF59E0B,
        }
    }
}

/// Handle controlling a live click-through border indicator. Automatically hides on drop.
pub struct CaptureBorderHandle {
    running: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl CaptureBorderHandle {
    /// Spawn a live click-through border around `region` when supported by the host display server.
    /// Returns `None` if unsupported (e.g. GNOME Wayland without layer-shell).
    pub fn spawn(region: Rectangle, style: BorderStyle) -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            if !region_platform::DisplayServer::detect().is_wayland() {
                return Self::spawn_x11(region, style);
            }
            debug!(
                "[Overlay] Wayland border requested for {:?} ({:?}); \
                 deferred to compositor indicator / HUD to avoid self-capture",
                region, style
            );
            None
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (region, style);
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn spawn_x11(region: Rectangle, style: BorderStyle) -> Option<Self> {
        use x11rb::connection::Connection;
        use x11rb::protocol::shape::{self, ConnectionExt as ShapeExt};
        use x11rb::protocol::xproto::{
            ConnectionExt, CreateWindowAux, EventMask, WindowClass,
        };

        let running = Arc::new(AtomicBool::new(true));
        let running_cl = Arc::clone(&running);

        let handle = thread::Builder::new()
            .name("region-border-x11".into())
            .spawn(move || {
                let Ok((conn, screen_num)) = x11rb::connect(None) else {
                    return;
                };
                let screen = &conn.setup().roots[screen_num];
                let thickness: i16 = 3;
                let color = style.rgb_u32();

                // 4 thin override-redirect bars placed OUTSIDE the captured region
                let bars = [
                    // Top bar
                    (
                        (region.x as i16).saturating_sub(thickness),
                        (region.y as i16).saturating_sub(thickness),
                        (region.width as u16).saturating_add((thickness * 2) as u16),
                        thickness as u16,
                    ),
                    // Bottom bar
                    (
                        (region.x as i16).saturating_sub(thickness),
                        (region.y + region.height as i32) as i16,
                        (region.width as u16).saturating_add((thickness * 2) as u16),
                        thickness as u16,
                    ),
                    // Left bar
                    (
                        (region.x as i16).saturating_sub(thickness),
                        region.y as i16,
                        thickness as u16,
                        region.height as u16,
                    ),
                    // Right bar
                    (
                        (region.x + region.width as i32) as i16,
                        region.y as i16,
                        thickness as u16,
                        region.height as u16,
                    ),
                ];

                let mut windows = Vec::with_capacity(4);
                for (bx, by, bw, bh) in bars {
                    if bw == 0 || bh == 0 {
                        continue;
                    }
                    let Ok(win) = conn.generate_id() else { continue };
                    let aux = CreateWindowAux::new()
                        .background_pixel(color)
                        .override_redirect(1)
                        .event_mask(EventMask::EXPOSURE);
                    if conn
                        .create_window(
                            x11rb::COPY_DEPTH_FROM_PARENT,
                            win,
                            screen.root,
                            bx,
                            by,
                            bw,
                            bh,
                            0,
                            WindowClass::INPUT_OUTPUT,
                            0,
                            &aux,
                        )
                        .is_ok()
                    {
                        // Make the bar 100% click-through via empty XShape input region
                        let _ = conn.shape_rectangles(
                            shape::SO::SET,
                            shape::SK::INPUT,
                            x11rb::protocol::xproto::ClipOrdering::UNSORTED,
                            win,
                            0,
                            0,
                            &[],
                        );
                        let _ = conn.map_window(win);
                        windows.push(win);
                    }
                }
                let _ = conn.flush();

                while running_cl.load(Ordering::Relaxed) {
                    thread::sleep(std::time::Duration::from_millis(80));
                }

                for win in windows {
                    let _ = conn.destroy_window(win);
                }
                let _ = conn.flush();
            })
            .ok()?;

        Some(Self {
            running,
            thread: Some(handle),
        })
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(th) = self.thread.take() {
            let _ = th.join();
        }
    }
}

impl Drop for CaptureBorderHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
