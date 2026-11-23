//! Branded Pause / "Be Right Back" Screen & FFmpeg Looping Video Player.
//!
//! When the user pauses sharing from the HUD (or IPC/Plasmoid), the Wayland Portal /
//! PipeWire capture session stays 100% alive in the background while the mirror window
//! (and Virtual Camera if active) stops rendering desktop pixels and instead displays:
//! - **Message mode**: clean broadcast card with the embedded Region to Share logo and
//!   a customizable message.
//! - **Video mode**: seamlessly looped video decoded via FFmpeg (`-stream_loop -1 -re`)
//!   with a subtle Region to Share logo badge, gracefully falling back to the branded
//!   message card if the video cannot be opened.

use crate::theme::Palette;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Rounding, Stroke, Ui};
use log::{debug, warn};
use parking_lot::Mutex as FastMutex;
use region_capture::{Frame, FrameData};
use region_core::{PixelFormat, Rectangle};
use rust_i18n::t;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;

/// Embedded 128×128 Region to Share application logo PNG.
const LOGO_PNG_BYTES: &[u8] = include_bytes!("../../../images/region-to-share-128.png");

/// Cached decoded RGBA8888 pixels `(width, height, rgba_bytes)` of the embedded logo.
static DECODED_LOGO: OnceLock<Option<(u32, u32, Vec<u8>)>> = OnceLock::new();

/// Decode the embedded Region to Share logo PNG once into RGBA8888 pixels.
pub fn embedded_logo_rgba() -> Option<&'static (u32, u32, Vec<u8>)> {
    DECODED_LOGO
        .get_or_init(|| decode_png_rgba(LOGO_PNG_BYTES))
        .as_ref()
}

/// Build an `egui::ColorImage` from the embedded Region to Share logo.
pub fn embedded_logo_color_image() -> Option<egui::ColorImage> {
    let (w, h, rgba) = embedded_logo_rgba()?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [*w as usize, *h as usize],
        rgba,
    ))
}

fn decode_png_rgba(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let buf_size = reader.output_buffer_size()?;
    let mut raw = vec![0u8; buf_size];
    let info = reader.next_frame(&mut raw).ok()?;
    let data = &raw[..info.buffer_size()];
    let w = info.width;
    let h = info.height;
    let count = (w as usize).saturating_mul(h as usize);

    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(count * 4);
            for px in data.chunks_exact(3).take(count) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(count * 4);
            for px in data.chunks_exact(2).take(count) {
                out.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(count * 4);
            for &g in data.iter().take(count) {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        _ => return None,
    };

    Some((w, h, rgba))
}

/// Generate a crisp dark-slate RGBA8888 frame with the Region to Share logo centered,
/// used to feed PipeWire / v4l2loopback Virtual Camera while paused in Message mode so
/// webcam viewers never see a frozen desktop image.
pub fn build_pause_logo_frame(width: u32, height: u32) -> Frame {
    let w = width.clamp(64, 3840);
    let h = height.clamp(64, 2160);
    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];

    // Fill studio-dark background (#0D1017)
    for px in buf.chunks_exact_mut(4) {
        px[0] = 0x0D;
        px[1] = 0x10;
        px[2] = 0x17;
        px[3] = 0xFF;
    }

    // Alpha-blend centered Region to Share logo scaled cleanly to ~28% of min dimension
    if let Some((lw, lh, lrgba)) = embedded_logo_rgba() {
        let target_side = ((w.min(h) as f32) * 0.28).clamp(48.0, 192.0) as u32;
        let dst_w = target_side.min(w);
        let dst_h = target_side.min(h);
        let ox = (w - dst_w) / 2;
        let oy = (h - dst_h) / 2;

        for dy in 0..dst_h {
            let sy = ((dy as u64 * *lh as u64) / dst_h.max(1) as u64).min(lh.saturating_sub(1) as u64) as usize;
            for dx in 0..dst_w {
                let sx = ((dx as u64 * *lw as u64) / dst_w.max(1) as u64).min(lw.saturating_sub(1) as u64) as usize;
                let s_idx = (sy * *lw as usize + sx) * 4;
                let sa = lrgba[s_idx + 3] as u32;
                if sa == 0 {
                    continue;
                }
                let d_idx = (((oy + dy) * w + (ox + dx)) as usize) * 4;
                let inv = 255 - sa;
                buf[d_idx] = ((lrgba[s_idx] as u32 * sa + buf[d_idx] as u32 * inv) / 255) as u8;
                buf[d_idx + 1] =
                    ((lrgba[s_idx + 1] as u32 * sa + buf[d_idx + 1] as u32 * inv) / 255) as u8;
                buf[d_idx + 2] =
                    ((lrgba[s_idx + 2] as u32 * sa + buf[d_idx + 2] as u32 * inv) / 255) as u8;
                buf[d_idx + 3] = 255;
            }
        }
    }

    Frame::new(
        w,
        h,
        PixelFormat::RGBA8888,
        FrameData::from_buffer(buf),
        Rectangle::new(0, 0, w, h),
        0,
    )
}

/// Render the branded Pause / BRB message screen inside `rect` (100% opaque — never leaks desktop).
pub fn render_pause_message_screen(
    ui: &mut Ui,
    rect: Rect,
    palette: Palette,
    logo_tex: Option<&egui::TextureHandle>,
    custom_message: &str,
) {
    let p = ui.painter();
    let bg = Color32::from_rgb(13, 16, 23);
    p.rect_filled(rect, 0.0, bg);

    let min_dim = rect.width().min(rect.height()).max(80.0);
    let t_sec = ui.input(|i| i.time);
    let pulse = ((t_sec * 2.2).sin() as f32) * 0.5 + 0.5;
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(60));

    let logo_side = (min_dim * 0.24).clamp(40.0, 112.0);
    let center_x = rect.center().x;
    let logo_center = pos2(center_x, rect.center().y - (min_dim * 0.13).clamp(24.0, 62.0));

    // Soft animated accent halo behind the logo
    p.circle_filled(
        logo_center,
        logo_side * (0.78 + pulse * 0.12),
        Palette::tint(palette.accent_share, (22.0 + pulse * 14.0) as u8),
    );
    p.circle_stroke(
        logo_center,
        logo_side * (0.64 + pulse * 0.05),
        Stroke::new(1.5_f32, Palette::tint(palette.accent_share, 95)),
    );

    if let Some(tex) = logo_tex {
        let logo_rect = Rect::from_center_size(logo_center, vec2(logo_side, logo_side));
        p.image(
            tex.id(),
            logo_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    // Compact amber "PAUSE" status pill
    let pill_y = logo_center.y + logo_side * 0.5 + (min_dim * 0.055).clamp(14.0, 26.0);
    let badge_text = t!("hud.paused").to_string();
    let badge_font = FontId::monospace((min_dim * 0.032).clamp(10.5, 13.5));
    let badge_galley = ui.fonts(|f| f.layout_no_wrap(badge_text.clone(), badge_font.clone(), palette.accent_warn));
    let badge_w = 24.0 + badge_galley.size().x + 12.0;
    let badge_h = (badge_galley.size().y + 10.0).max(22.0);
    let badge_rect = Rect::from_center_size(pos2(center_x, pill_y), vec2(badge_w, badge_h));
    p.rect_filled(badge_rect, Rounding::same(badge_h * 0.5), Palette::tint(palette.accent_warn, 36));
    p.rect_stroke(
        badge_rect,
        Rounding::same(badge_h * 0.5),
        Stroke::new(1.0_f32, Palette::tint(palette.accent_warn, 125)),
    );
    p.circle_filled(
        pos2(badge_rect.left() + 13.0, badge_rect.center().y),
        3.5 + pulse * 1.2,
        palette.accent_warn,
    );
    p.text(
        pos2(badge_rect.left() + 23.0, badge_rect.center().y),
        Align2::LEFT_CENTER,
        badge_text,
        badge_font,
        Color32::WHITE,
    );

    // Main configurable message (centered & cleanly wrapped)
    let msg = if custom_message.trim().is_empty() {
        t!("pause.default_message").to_string()
    } else {
        custom_message.trim().to_string()
    };
    let msg_font = FontId::proportional((min_dim * 0.052).clamp(14.0, 28.0));
    let max_text_w = (rect.width() * 0.82).max(120.0);
    let msg_galley = ui.fonts(|f| f.layout(msg, msg_font, Color32::from_rgb(241, 245, 249), max_text_w));
    let msg_top = badge_rect.bottom() + (min_dim * 0.04).clamp(10.0, 22.0);
    let msg_pos = pos2(center_x - msg_galley.size().x * 0.5, msg_top);
    p.galley(msg_pos, msg_galley, Color32::WHITE);

    // Subtle Region to Share brand footer
    let footer_y = (rect.bottom() - (min_dim * 0.055).clamp(16.0, 28.0)).max(msg_top + 28.0);
    if footer_y < rect.bottom() - 6.0 {
        p.text(
            pos2(center_x, footer_y),
            Align2::CENTER_CENTER,
            "Region to Share",
            FontId::proportional((min_dim * 0.03).clamp(10.5, 13.5)),
            Color32::from_rgba_unmultiplied(148, 163, 184, 175),
        );
    }
}

/// Render a subtle Region to Share corner logo badge on top of a looping pause video.
pub fn render_pause_video_brand_badge(
    ui: &mut Ui,
    rect: Rect,
    palette: Palette,
    logo_tex: Option<&egui::TextureHandle>,
) {
    if rect.width() < 140.0 || rect.height() < 90.0 {
        return;
    }
    let p = ui.painter();
    let font = FontId::proportional(11.5);
    let label = "Region to Share";
    let galley = ui.fonts(|f| f.layout_no_wrap(label.to_string(), font.clone(), Color32::WHITE));
    let icon_sz = if logo_tex.is_some() { 18.0 } else { 0.0 };
    let pad_x = 10.0;
    let gap = if logo_tex.is_some() { 6.0 } else { 0.0 };
    let w = pad_x + icon_sz + gap + galley.size().x + pad_x;
    let h = 28.0;
    let badge_rect = Rect::from_min_size(
        pos2(rect.right() - w - 12.0, rect.bottom() - h - 12.0),
        vec2(w, h),
    );
    p.rect_filled(
        badge_rect,
        Rounding::same(9.0),
        Color32::from_rgba_unmultiplied(13, 16, 23, 195),
    );
    p.rect_stroke(
        badge_rect,
        Rounding::same(9.0),
        Stroke::new(1.0_f32, Palette::tint(palette.border_strong, 160)),
    );
    let mut text_x = badge_rect.left() + pad_x;
    if let Some(tex) = logo_tex {
        let icon_rect = Rect::from_center_size(
            pos2(badge_rect.left() + pad_x + icon_sz * 0.5, badge_rect.center().y),
            vec2(icon_sz, icon_sz),
        );
        p.image(
            tex.id(),
            icon_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        text_x = icon_rect.right() + gap;
    }
    p.text(
        pos2(text_x, badge_rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        Color32::from_rgba_unmultiplied(241, 245, 249, 230),
    );
}

/// Compute an even target resolution for decoding the pause video without burning CPU on 4K+.
pub fn pause_video_decode_size(region_w: u32, region_h: u32) -> (u32, u32) {
    let mut w = region_w.max(64) as f64;
    let mut h = region_h.max(64) as f64;
    let max_w = 1920.0;
    let max_h = 1080.0;
    if w > max_w || h > max_h {
        let scale = (max_w / w).min(max_h / h);
        w *= scale;
        h *= scale;
    }
    let ew = ((w.round() as u32) & !1).clamp(64, 1920);
    let eh = ((h.round() as u32) & !1).clamp(64, 1080);
    (ew, eh)
}

/// Seamless looping FFmpeg video decoder feeding RGBA8888 `Frame`s while the stream is paused.
pub struct PauseVideoPlayer {
    running: Arc<AtomicBool>,
    child: Arc<FastMutex<Option<Child>>>,
    thread: Option<thread::JoinHandle<()>>,
    latest_for_vcam: Arc<FastMutex<Option<Arc<Frame>>>>,
    seq: Arc<AtomicU64>,
}

impl PauseVideoPlayer {
    /// Start decoding `video_path` in an infinite loop at its native framerate (`-stream_loop -1 -re`).
    pub fn start(
        video_path: &Path,
        target_w: u32,
        target_h: u32,
        ui_frame_slot: Arc<FastMutex<Option<Arc<Frame>>>>,
        egui_ctx: egui::Context,
    ) -> Result<Self, String> {
        if !video_path.is_file() {
            return Err(format!("Pause video not found: {}", video_path.display()));
        }

        let (w, h) = pause_video_decode_size(target_w, target_h);
        let vf = format!(
            "scale={w}:{h}:force_original_aspect_ratio=decrease,\
             pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=0x0d1017,setsar=1"
        );

        let mut child = Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-stream_loop",
                "-1",
                "-re",
                "-i",
            ])
            .arg(video_path)
            .args([
                "-an",
                "-sn",
                "-dn",
                "-vf",
                &vf,
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to launch FFmpeg for pause video: {e}"))?;

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| "FFmpeg stdout unavailable".to_string())?;

        let running = Arc::new(AtomicBool::new(true));
        let running_cl = Arc::clone(&running);
        let child_arc = Arc::new(FastMutex::new(Some(child)));
        let child_cl = Arc::clone(&child_arc);
        let latest_for_vcam: Arc<FastMutex<Option<Arc<Frame>>>> = Arc::new(FastMutex::new(None));
        let vcam_cl = Arc::clone(&latest_for_vcam);
        let seq = Arc::new(AtomicU64::new(0));
        let seq_cl = Arc::clone(&seq);

        let frame_bytes = (w as usize) * (h as usize) * 4;
        let path_disp = video_path.display().to_string();

        let thread = thread::Builder::new()
            .name("pause-video-loop".into())
            .spawn(move || {
                debug!("[PauseVideo] Streaming {path_disp} at {w}x{h} (loop)");
                let mut raw = vec![0u8; frame_bytes];
                while running_cl.load(Ordering::Relaxed) {
                    match stdout.read_exact(&mut raw) {
                        Ok(()) => {
                            let s = seq_cl.fetch_add(1, Ordering::Relaxed) + 1;
                            let frame = Arc::new(Frame::new(
                                w,
                                h,
                                PixelFormat::RGBA8888,
                                FrameData::from_buffer(raw.clone()),
                                Rectangle::new(0, 0, w, h),
                                s,
                            ));
                            *ui_frame_slot.lock() = Some(Arc::clone(&frame));
                            *vcam_cl.lock() = Some(frame);
                            egui_ctx.request_repaint();
                        }
                        Err(_) => {
                            if running_cl.load(Ordering::Relaxed) {
                                warn!("[PauseVideo] FFmpeg stream ended unexpectedly for {path_disp}");
                            }
                            break;
                        }
                    }
                }
                if let Some(mut c) = child_cl.lock().take() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            })
            .map_err(|e| format!("Failed to spawn pause video thread: {e}"))?;

        Ok(Self {
            running,
            child: child_arc,
            thread: Some(thread),
            latest_for_vcam,
            seq,
        })
    }

    /// Take the most recently decoded pause video frame for Virtual Camera forwarding.
    pub fn take_vcam_frame(&self) -> Option<Arc<Frame>> {
        self.latest_for_vcam.lock().take()
    }

    /// Number of frames decoded so far.
    pub fn frame_count(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }

    /// Immediately kill FFmpeg and join the background decoder thread.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(mut c) = self.child.lock().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(th) = self.thread.take() {
            let _ = th.join();
        }
    }
}

impl Drop for PauseVideoPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_logo_decodes_cleanly() {
        let (w, h, rgba) = embedded_logo_rgba().expect("embedded logo PNG should decode");
        assert_eq!(*w, 128);
        assert_eq!(*h, 128);
        assert_eq!(rgba.len(), 128 * 128 * 4);
    }

    #[test]
    fn test_build_pause_logo_frame_dimensions() {
        let frame = build_pause_logo_frame(640, 360);
        assert_eq!(frame.width, 640);
        assert_eq!(frame.height, 360);
        assert_eq!(frame.format, PixelFormat::RGBA8888);
        assert_eq!(frame.data.as_buffer().unwrap().len(), 640 * 360 * 4);
    }

    #[test]
    fn test_pause_video_decode_size_clamps_to_even_1080p() {
        assert_eq!(pause_video_decode_size(1281, 721), (1280, 720));
        assert_eq!(pause_video_decode_size(3840, 2160), (1920, 1080));
        assert_eq!(pause_video_decode_size(20, 20), (64, 64));
    }
}
