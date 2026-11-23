//! PipeWire Virtual Camera output (`Video/Source`, `Camera`).
//!
//! Exposes the captured screen region as a native PipeWire camera node visible
//! directly in Firefox, Chrome/Chromium, Google Meet, Discord, Zoom, and OBS —
//! with zero kernel module (`v4l2loopback`) required.

use region_capture::{CaptureError, Frame, Result};
use std::sync::Arc;

/// Configuration for the PipeWire Virtual Camera node.
#[derive(Debug, Clone)]
pub struct VirtualCameraConfig {
    pub node_name: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for VirtualCameraConfig {
    fn default() -> Self {
        Self {
            node_name: "region-to-share-vcam".to_string(),
            description: "Region to Share (Virtual Camera)".to_string(),
            width: 1280,
            height: 720,
            fps: 30,
        }
    }
}

#[cfg(feature = "pipewire")]
mod vcam_impl {
    use super::*;
    use crossbeam_channel::{self as channel, Receiver, Sender};
    use log::{debug, error, info, warn};
    use region_core::PixelFormat;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    struct QuitPtr(*mut pipewire::sys::pw_main_loop);
    unsafe impl Send for QuitPtr {}

    struct V4l2Bridge {
        frame_tx: Option<Sender<Arc<Frame>>>,
        writer: Option<thread::JoinHandle<()>>,
        child: Option<std::process::Child>,
        device_path: std::path::PathBuf,
    }

    impl V4l2Bridge {
        fn try_start(width: u32, height: u32, fps: u32) -> Option<Self> {
            use std::io::Write;
            use std::process::{Command, Stdio};

            let dev = region_platform::detect_v4l2loopback_device()?;
            let w = (width / 2 * 2).max(2);
            let h = (height / 2 * 2).max(2);
            let fps = fps.clamp(1, 60);
            let expected_bytes = (w as usize) * (h as usize) * 4;

            let mut child = Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "rawvideo",
                    "-pixel_format",
                    "bgra",
                    "-video_size",
                    &format!("{w}x{h}"),
                    "-framerate",
                    &fps.to_string(),
                    "-i",
                    "pipe:0",
                    "-vf",
                    "format=yuv420p",
                    "-f",
                    "v4l2",
                    &dev.to_string_lossy(),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;

            let mut stdin = child.stdin.take()?;
            let (frame_tx, frame_rx) = channel::bounded::<Arc<Frame>>(2);
            let tick = Duration::from_secs_f64(1.0 / fps as f64);

            let writer = thread::Builder::new()
                .name("region-v4l2-bridge".into())
                .spawn(move || {
                    let mut last: Option<Arc<Frame>> = None;
                    let mut scratch = vec![0u8; expected_bytes];
                    loop {
                        match frame_rx.recv_timeout(tick) {
                            Ok(f) => {
                                last = Some(f);
                            }
                            Err(channel::RecvTimeoutError::Timeout) => {}
                            Err(channel::RecvTimeoutError::Disconnected) => break,
                        }
                        if let Some(ref f) = last {
                            if let Some(bytes) = f.data.as_buffer() {
                                let swap_rb = matches!(f.format, PixelFormat::RGBA8888);
                                blit_frame(bytes, f.width, f.height, &mut scratch, w, h, swap_rb);
                                if stdin.write_all(&scratch).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    let _ = stdin.flush();
                })
                .ok()?;

            info!(
                "[VirtualCam] Also mirroring to v4l2loopback device {} ({}x{} @ {}fps) — visible in Chrome/Meet/Discord with zero flags!",
                dev.display(),
                w,
                h,
                fps
            );

            Some(Self {
                frame_tx: Some(frame_tx),
                writer: Some(writer),
                child: Some(child),
                device_path: dev,
            })
        }

        fn push(&self, frame: Arc<Frame>) {
            if let Some(ref tx) = self.frame_tx {
                let _ = tx.try_send(frame);
            }
        }

        fn stop(&mut self) {
            drop(self.frame_tx.take());
            if let Some(w) = self.writer.take() {
                let _ = w.join();
            }
            if let Some(mut c) = self.child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }

    /// Live PipeWire virtual camera producer (`Video/Source`, `Camera`) with automatic
    /// `v4l2loopback` (`/dev/video*`) mirroring when loaded on the host.
    pub struct VirtualCameraSink {
        frame_tx: Sender<Arc<Frame>>,
        active: Arc<AtomicBool>,
        quit: Option<QuitPtr>,
        thread: Option<thread::JoinHandle<()>>,
        v4l2: Option<V4l2Bridge>,
    }

    impl VirtualCameraSink {
        pub fn start(config: VirtualCameraConfig) -> Result<Self> {
            let v4l2 = V4l2Bridge::try_start(config.width, config.height, config.fps);
            let (frame_tx, frame_rx) = channel::bounded::<Arc<Frame>>(2);
            let (quit_tx, quit_rx) = channel::bounded::<QuitPtr>(1);
            let active = Arc::new(AtomicBool::new(true));
            let active_clone = Arc::clone(&active);

            let thread = thread::spawn(move || {
                if let Err(e) = run_virtual_camera_loop(config, frame_rx, quit_tx, active_clone) {
                    error!("[VirtualCam] PipeWire loop failed: {}", e);
                }
            });

            let quit = quit_rx.recv_timeout(Duration::from_secs(3)).ok();
            if quit.is_none() {
                active.store(false, Ordering::Relaxed);
                return Err(CaptureError::InitFailed(
                    "Timed out initializing PipeWire virtual camera".into(),
                ));
            }

            Ok(Self {
                frame_tx,
                active,
                quit,
                thread: Some(thread),
                v4l2,
            })
        }

        /// Returns the mirrored `/dev/video*` path if `v4l2loopback` is active on this host.
        pub fn v4l2_device(&self) -> Option<&std::path::Path> {
            self.v4l2.as_ref().map(|v| v.device_path.as_path())
        }

        /// Push the latest captured frame to the virtual camera (drops oldest if busy).
        pub fn push_frame(&self, frame: Arc<Frame>) {
            if let Some(ref v4l2) = self.v4l2 {
                v4l2.push(Arc::clone(&frame));
            }
            let _ = self.frame_tx.try_send(frame);
        }

        pub fn is_active(&self) -> bool {
            self.active.load(Ordering::Relaxed)
        }

        pub fn stop(&mut self) {
            self.active.store(false, Ordering::Relaxed);
            if let Some(ref mut v4l2) = self.v4l2 {
                v4l2.stop();
            }
            self.v4l2 = None;
            if let Some(q) = self.quit.take() {
                unsafe { pipewire::sys::pw_main_loop_quit(q.0) };
            }
            if let Some(th) = self.thread.take() {
                let _ = th.join();
            }
        }
    }

    impl Drop for VirtualCameraSink {
        fn drop(&mut self) {
            self.stop();
        }
    }

    /// Negotiated pixel layout for the PipeWire Virtual Camera output.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum OutputPixelFormat {
        Yuy2,
        I420,
        Bgra,
        Rgba,
    }

    impl OutputPixelFormat {
        pub(super) fn from_spa(fmt: libspa::param::video::VideoFormat) -> Self {
            use libspa::param::video::VideoFormat;
            match fmt {
                VideoFormat::YUY2 => Self::Yuy2,
                VideoFormat::I420 => Self::I420,
                VideoFormat::RGBA | VideoFormat::RGBx => Self::Rgba,
                _ => Self::Bgra,
            }
        }

        pub(super) fn buffer_metrics(self, width: u32, height: u32) -> (i32, usize) {
            let w = width as usize;
            let h = height as usize;
            match self {
                Self::Yuy2 => ((w * 2) as i32, w * h * 2),
                Self::I420 => (w as i32, w * h * 3 / 2),
                Self::Bgra | Self::Rgba => ((w * 4) as i32, w * h * 4),
            }
        }
    }

    /// Build `SPA_PARAM_EnumFormat` advertising WebRTC-native YUV (`YUY2`, `I420`)
    /// plus 32-bit RGB/BGR formats so Firefox, Chrome/Meet, OBS, and PipeWire
    /// consumers negotiate cleanly.
    pub(super) fn build_output_format_pod(buf: &mut Vec<u8>, width: u32, height: u32, fps: u32) -> bool {
        use libspa::param::format::{FormatProperties, MediaSubtype, MediaType};
        use libspa::param::video::VideoFormat;
        use libspa::param::ParamType;
        use libspa::pod::serialize::PodSerializer;
        use libspa::pod::{ChoiceValue, Object, Property, PropertyFlags, Value};
        use libspa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle as SpaRect, SpaTypes};

        buf.clear();
        let fps = fps.clamp(1, 120);

        let obj = Object {
            type_: SpaTypes::ObjectParamFormat.as_raw(),
            id: ParamType::EnumFormat.as_raw(),
            properties: vec![
                Property {
                    key: FormatProperties::MediaType.as_raw(),
                    flags: PropertyFlags::empty(),
                    value: Value::Id(Id(MediaType::Video.as_raw())),
                },
                Property {
                    key: FormatProperties::MediaSubtype.as_raw(),
                    flags: PropertyFlags::empty(),
                    value: Value::Id(Id(MediaSubtype::Raw.as_raw())),
                },
                Property {
                    key: FormatProperties::VideoFormat.as_raw(),
                    flags: PropertyFlags::empty(),
                    value: Value::Choice(ChoiceValue::Id(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Enum {
                            default: Id(VideoFormat::YUY2.as_raw()),
                            alternatives: vec![
                                Id(VideoFormat::I420.as_raw()),
                                Id(VideoFormat::BGRA.as_raw()),
                                Id(VideoFormat::RGBA.as_raw()),
                                Id(VideoFormat::BGRx.as_raw()),
                                Id(VideoFormat::RGBx.as_raw()),
                            ],
                        },
                    ))),
                },
                Property {
                    key: FormatProperties::VideoSize.as_raw(),
                    flags: PropertyFlags::empty(),
                    value: Value::Rectangle(SpaRect { width, height }),
                },
                Property {
                    key: FormatProperties::VideoFramerate.as_raw(),
                    flags: PropertyFlags::empty(),
                    value: Value::Fraction(Fraction { num: fps, denom: 1 }),
                },
            ],
        };

        let mut cursor = std::io::Cursor::new(&mut *buf);
        PodSerializer::serialize(&mut cursor, &Value::Object(obj)).is_ok()
    }

    /// Build `SPA_PARAM_Buffers` returned inside `param_changed(ParamType::Format)`.
    /// Without this response, PipeWire/WirePlumber/browsers refuse to link the camera node!
    pub(super) fn build_buffers_pod(buf: &mut Vec<u8>, size: i32, stride: i32) -> bool {
        use libspa::param::ParamType;
        use libspa::pod::serialize::PodSerializer;
        use libspa::pod::{ChoiceValue, Object, Property, PropertyFlags, Value};
        use libspa::sys::{
            SPA_DATA_MemFd, SPA_DATA_MemPtr, SPA_PARAM_BUFFERS_align, SPA_PARAM_BUFFERS_blocks,
            SPA_PARAM_BUFFERS_buffers, SPA_PARAM_BUFFERS_dataType, SPA_PARAM_BUFFERS_size,
            SPA_PARAM_BUFFERS_stride, SPA_TYPE_OBJECT_ParamBuffers,
        };
        use libspa::utils::{Choice, ChoiceEnum, ChoiceFlags};

        buf.clear();
        let data_types = ((1u32 << SPA_DATA_MemFd) | (1u32 << SPA_DATA_MemPtr)) as i32;

        let obj = Object {
            type_: SPA_TYPE_OBJECT_ParamBuffers,
            id: ParamType::Buffers.as_raw(),
            properties: vec![
                Property {
                    key: SPA_PARAM_BUFFERS_buffers,
                    flags: PropertyFlags::empty(),
                    value: Value::Choice(ChoiceValue::Int(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Range {
                            default: 4,
                            min: 2,
                            max: 8,
                        },
                    ))),
                },
                Property {
                    key: SPA_PARAM_BUFFERS_blocks,
                    flags: PropertyFlags::empty(),
                    value: Value::Int(1),
                },
                Property {
                    key: SPA_PARAM_BUFFERS_size,
                    flags: PropertyFlags::empty(),
                    value: Value::Int(size.max(4)),
                },
                Property {
                    key: SPA_PARAM_BUFFERS_stride,
                    flags: PropertyFlags::empty(),
                    value: Value::Int(stride.max(4)),
                },
                Property {
                    key: SPA_PARAM_BUFFERS_align,
                    flags: PropertyFlags::empty(),
                    value: Value::Int(16),
                },
                Property {
                    key: SPA_PARAM_BUFFERS_dataType,
                    flags: PropertyFlags::empty(),
                    value: Value::Int(data_types),
                },
            ],
        };

        let mut cursor = std::io::Cursor::new(&mut *buf);
        PodSerializer::serialize(&mut cursor, &Value::Object(obj)).is_ok()
    }

    #[inline(always)]
    fn rgb_to_y(r: i32, g: i32, b: i32) -> u8 {
        (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(16, 235) as u8
    }

    #[inline(always)]
    fn rgb_to_u(r: i32, g: i32, b: i32) -> u8 {
        (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(16, 240) as u8
    }

    #[inline(always)]
    fn rgb_to_v(r: i32, g: i32, b: i32) -> u8 {
        (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(16, 240) as u8
    }

    #[inline(always)]
    fn sample_rgb(
        src: &[u8],
        src_w: usize,
        src_h: usize,
        x: usize,
        y: usize,
        src_is_rgba: bool,
    ) -> (i32, i32, i32) {
        if x >= src_w || y >= src_h {
            return (0, 0, 0);
        }
        let off = (y * src_w + x) * 4;
        if off + 3 >= src.len() {
            return (0, 0, 0);
        }
        if src_is_rgba {
            (src[off] as i32, src[off + 1] as i32, src[off + 2] as i32)
        } else {
            (src[off + 2] as i32, src[off + 1] as i32, src[off] as i32)
        }
    }

    /// Convert and copy a 32-bit BGRA/RGBA capture frame into the negotiated PipeWire camera format
    /// (`YUY2`, `I420`, `BGRA`, or `RGBA`), safely handling any dimension mismatch without shear.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn blit_frame_formatted(
        src: &[u8],
        src_w: u32,
        src_h: u32,
        src_is_rgba: bool,
        dst: &mut [u8],
        dst_w: u32,
        dst_h: u32,
        out_fmt: OutputPixelFormat,
    ) {
        let sw = src_w as usize;
        let sh = src_h as usize;
        let dw = (dst_w as usize) & !1;
        let dh = (dst_h as usize) & !1;

        match out_fmt {
            OutputPixelFormat::Bgra | OutputPixelFormat::Rgba => {
                let swap_rb = src_is_rgba != matches!(out_fmt, OutputPixelFormat::Rgba);
                blit_frame(src, src_w, src_h, dst, dst_w, dst_h, swap_rb);
            }
            OutputPixelFormat::Yuy2 => {
                let total = dw * dh * 2;
                if dst.len() < total {
                    return;
                }
                for y in 0..dh {
                    let dst_row = &mut dst[y * dw * 2..(y + 1) * dw * 2];
                    for pair in 0..(dw / 2) {
                        let x0 = pair * 2;
                        let (r0, g0, b0) = sample_rgb(src, sw, sh, x0, y, src_is_rgba);
                        let (r1, g1, b1) = sample_rgb(src, sw, sh, x0 + 1, y, src_is_rgba);
                        let y0 = rgb_to_y(r0, g0, b0);
                        let y1 = rgb_to_y(r1, g1, b1);
                        let u = rgb_to_u((r0 + r1) >> 1, (g0 + g1) >> 1, (b0 + b1) >> 1);
                        let v = rgb_to_v((r0 + r1) >> 1, (g0 + g1) >> 1, (b0 + b1) >> 1);
                        let o = pair * 4;
                        dst_row[o] = y0;
                        dst_row[o + 1] = u;
                        dst_row[o + 2] = y1;
                        dst_row[o + 3] = v;
                    }
                }
            }
            OutputPixelFormat::I420 => {
                let y_len = dw * dh;
                let uv_w = dw / 2;
                let uv_h = dh / 2;
                let uv_len = uv_w * uv_h;
                if dst.len() < y_len + uv_len * 2 {
                    return;
                }
                let (y_plane, uv_rest) = dst.split_at_mut(y_len);
                let (u_plane, v_plane_full) = uv_rest.split_at_mut(uv_len);
                let v_plane = &mut v_plane_full[..uv_len];

                for by in 0..uv_h {
                    let y0 = by * 2;
                    let y1 = y0 + 1;
                    for bx in 0..uv_w {
                        let x0 = bx * 2;
                        let x1 = x0 + 1;
                        let (r00, g00, b00) = sample_rgb(src, sw, sh, x0, y0, src_is_rgba);
                        let (r10, g10, b10) = sample_rgb(src, sw, sh, x1, y0, src_is_rgba);
                        let (r01, g01, b01) = sample_rgb(src, sw, sh, x0, y1, src_is_rgba);
                        let (r11, g11, b11) = sample_rgb(src, sw, sh, x1, y1, src_is_rgba);

                        y_plane[y0 * dw + x0] = rgb_to_y(r00, g00, b00);
                        y_plane[y0 * dw + x1] = rgb_to_y(r10, g10, b10);
                        y_plane[y1 * dw + x0] = rgb_to_y(r01, g01, b01);
                        y_plane[y1 * dw + x1] = rgb_to_y(r11, g11, b11);

                        let ra = (r00 + r10 + r01 + r11) >> 2;
                        let ga = (g00 + g10 + g01 + g11) >> 2;
                        let ba = (b00 + b10 + b01 + b11) >> 2;
                        let uv_idx = by * uv_w + bx;
                        u_plane[uv_idx] = rgb_to_u(ra, ga, ba);
                        v_plane[uv_idx] = rgb_to_v(ra, ga, ba);
                    }
                }
            }
        }
    }

    /// Copy `src` (size `src_w x src_h`) into `dst` (size `dst_w x dst_h`),
    /// swapping R<->B channels if `swap_rb` is true and handling any size mismatch cleanly.
    pub(super) fn blit_frame(
        src: &[u8],
        src_w: u32,
        src_h: u32,
        dst: &mut [u8],
        dst_w: u32,
        dst_h: u32,
        swap_rb: bool,
    ) {
        let dst_stride = dst_w as usize * 4;
        let dst_total = (dst_stride * dst_h as usize).min(dst.len());
        let src_stride = src_w as usize * 4;

        if src_w == dst_w && src_h == dst_h && src.len() >= dst_total {
            if !swap_rb {
                dst[..dst_total].copy_from_slice(&src[..dst_total]);
            } else {
                for (d, s) in dst[..dst_total]
                    .chunks_exact_mut(4)
                    .zip(src[..dst_total].chunks_exact(4))
                {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = 255;
                }
            }
            return;
        }

        // Dimension mismatch (e.g. live re-crop): zero-fill background & copy overlapping rows
        dst[..dst_total].fill(0);
        let copy_w = src_w.min(dst_w) as usize;
        let copy_h = src_h.min(dst_h) as usize;
        let row_bytes = copy_w * 4;
        for y in 0..copy_h {
            let s_off = y * src_stride;
            let d_off = y * dst_stride;
            if s_off + row_bytes > src.len() || d_off + row_bytes > dst_total {
                break;
            }
            let s_row = &src[s_off..s_off + row_bytes];
            let d_row = &mut dst[d_off..d_off + row_bytes];
            if !swap_rb {
                d_row.copy_from_slice(s_row);
            } else {
                for (d, s) in d_row.chunks_exact_mut(4).zip(s_row.chunks_exact(4)) {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = 255;
                }
            }
        }
    }

    fn run_virtual_camera_loop(
        config: VirtualCameraConfig,
        frame_rx: Receiver<Arc<Frame>>,
        quit_tx: Sender<QuitPtr>,
        active: Arc<AtomicBool>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use pipewire as pw;

        pw::init();
        let mainloop = pw::main_loop::MainLoopBox::new(None)?;
        let _ = quit_tx.send(QuitPtr(mainloop.as_raw_ptr()));

        let context = pw::context::ContextBox::new(&mainloop.loop_(), None)?;
        let core = context.connect(None)?;

        let stream = pw::stream::StreamBox::new(
            &core,
            &config.node_name,
            pw::properties::properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Source",
                *pw::keys::MEDIA_ROLE => "Camera",
                *pw::keys::MEDIA_CLASS => "Video/Source",
                *pw::keys::NODE_DESCRIPTION => config.description.as_str(),
                "node.name" => config.node_name.as_str(),
                "node.nick" => "Region to Share",
                "node.pause-on-idle" => "false",
                "device.icon-name" => "camera-web",
                "priority.session" => "100",
            },
        )?;

        let width = (config.width.max(2) / 2 * 2).max(2);
        let height = (config.height.max(2) / 2 * 2).max(2);
        let fps = config.fps.clamp(1, 120);
        let frame_interval = Duration::from_nanos(1_000_000_000 / fps as u64);

        // Negotiated format state shared across single-threaded mainloop callbacks
        let neg_width = Rc::new(Cell::new(width));
        let neg_height = Rc::new(Cell::new(height));
        let neg_fmt = Rc::new(Cell::new(OutputPixelFormat::Yuy2));
        let is_streaming = Rc::new(Cell::new(false));

        // Timer driving the Video/Source clock whenever a consumer connects (`Streaming`)
        let stream_raw_ptr = stream.as_raw_ptr() as usize;
        let streaming_timer = Rc::clone(&is_streaming);
        let _timer = mainloop.loop_().add_timer(move |_expirations| {
            if streaming_timer.get() {
                unsafe {
                    pw::sys::pw_stream_trigger_process(stream_raw_ptr as *mut pw::sys::pw_stream);
                }
            }
        });
        let _ = _timer.update_timer(Some(frame_interval), Some(frame_interval));

        let streaming_state = Rc::clone(&is_streaming);
        let nw_param = Rc::clone(&neg_width);
        let nh_param = Rc::clone(&neg_height);
        let fmt_param = Rc::clone(&neg_fmt);

        let nw_proc = Rc::clone(&neg_width);
        let nh_proc = Rc::clone(&neg_height);
        let fmt_proc = Rc::clone(&neg_fmt);
        let mut last_frame: Option<Arc<Frame>> = None;

        let _listener = stream
            .add_local_listener_with_user_data(())
            .state_changed(move |_, _, old, new| {
                debug!("[VirtualCam] state {:?} -> {:?}", old, new);
                match new {
                    pw::stream::StreamState::Streaming => {
                        info!(
                            "[VirtualCam] Consumer connected — clocking frames at {} FPS",
                            fps
                        );
                        streaming_state.set(true);
                    }
                    pw::stream::StreamState::Paused
                    | pw::stream::StreamState::Unconnected
                    | pw::stream::StreamState::Error(_) => {
                        streaming_state.set(false);
                    }
                    _ => {}
                }
            })
            .param_changed(move |stream, _, id, param| {
                if id != libspa::param::ParamType::Format.as_raw() {
                    return;
                }
                let Some(pod) = param else { return };

                let mut video_info = libspa::param::video::VideoInfoRaw::new();
                if let Err(e) = video_info.parse(pod) {
                    warn!("[VirtualCam] Failed to parse negotiated format: {:?}", e);
                    return;
                }

                let size = video_info.size();
                let w = if size.width > 0 { (size.width / 2 * 2).max(2) } else { width };
                let h = if size.height > 0 { (size.height / 2 * 2).max(2) } else { height };
                let out_fmt = OutputPixelFormat::from_spa(video_info.format());
                nw_param.set(w);
                nh_param.set(h);
                fmt_param.set(out_fmt);

                let (stride, buf_size) = out_fmt.buffer_metrics(w, h);
                let mut buf_pod = Vec::with_capacity(256);
                if build_buffers_pod(&mut buf_pod, buf_size as i32, stride) {
                    if let Some(pod) = libspa::pod::Pod::from_bytes(&buf_pod) {
                        let mut params = [pod];
                        if let Err(e) = stream.update_params(&mut params) {
                            warn!("[VirtualCam] update_params(Buffers) failed: {:?}", e);
                        } else {
                            info!(
                                "[VirtualCam] Negotiated {:?} ({:?}) {}x{} (stride={}, bytes={}, buffers=4)",
                                video_info.format(),
                                out_fmt,
                                w,
                                h,
                                stride,
                                buf_size
                            );
                        }
                    }
                }
            })
            .process(move |stream, _| {
                // Drain to the most recent captured frame
                while let Ok(f) = frame_rx.try_recv() {
                    last_frame = Some(f);
                }

                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let datas = buffer.datas_mut();
                if datas.is_empty() {
                    return;
                }

                let dst_w = nw_proc.get().max(2);
                let dst_h = nh_proc.get().max(2);
                let out_fmt = fmt_proc.get();
                let (stride, expected_size) = out_fmt.buffer_metrics(dst_w, dst_h);

                let data_ref = &mut datas[0];
                if let Some(dst) = data_ref.data() {
                    let copy_len = expected_size.min(dst.len());
                    if let Some(ref frame) = last_frame {
                        if let Some(src_bytes) = frame.data.as_buffer() {
                            let src_is_rgba = matches!(frame.format, PixelFormat::RGBA8888);
                            blit_frame_formatted(
                                src_bytes,
                                frame.width,
                                frame.height,
                                src_is_rgba,
                                &mut dst[..copy_len],
                                dst_w,
                                dst_h,
                                out_fmt,
                            );
                        } else {
                            dst[..copy_len].fill(0);
                        }
                    } else {
                        dst[..copy_len].fill(0);
                    }
                    let chunk = data_ref.chunk_mut();
                    *chunk.offset_mut() = 0;
                    *chunk.stride_mut() = stride;
                    *chunk.size_mut() = copy_len as u32;
                }
            })
            .register()?;

        let mut fmt_buf = Vec::with_capacity(512);
        let mut params_refs: Vec<&pw::spa::pod::Pod> = Vec::new();
        if build_output_format_pod(&mut fmt_buf, width, height, fps) {
            if let Some(pod) = libspa::pod::Pod::from_bytes(&fmt_buf) {
                params_refs.push(pod);
            }
        }

        stream.connect(
            pw::spa::utils::Direction::Output,
            None,
            pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::DRIVER,
            &mut params_refs,
        )?;

        info!(
            "[VirtualCam] PipeWire Video/Source '{}' active ({}x{} @ {}fps)",
            config.description, width, height, fps
        );
        mainloop.run();
        active.store(false, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(not(feature = "pipewire"))]
mod vcam_impl {
    use super::*;

    pub struct VirtualCameraSink;

    impl VirtualCameraSink {
        pub fn start(_config: VirtualCameraConfig) -> Result<Self> {
            Err(CaptureError::NotAvailable(
                "PipeWire Virtual Camera requires building with the `pipewire` feature".into(),
            ))
        }

        pub fn v4l2_device(&self) -> Option<&std::path::Path> {
            None
        }

        pub fn push_frame(&self, _frame: Arc<Frame>) {}

        pub fn is_active(&self) -> bool {
            false
        }

        pub fn stop(&mut self) {}
    }
}

pub use vcam_impl::VirtualCameraSink;

#[cfg(all(test, feature = "pipewire"))]
mod tests {
    use super::vcam_impl::{
        blit_frame, blit_frame_formatted, build_buffers_pod, build_output_format_pod,
        OutputPixelFormat,
    };

    #[test]
    fn test_vcam_format_and_buffers_pods_serialize() {
        let mut fmt = Vec::new();
        assert!(build_output_format_pod(&mut fmt, 1280, 720, 30));
        assert!(libspa::pod::Pod::from_bytes(&fmt).is_some());

        let mut bufs = Vec::new();
        assert!(build_buffers_pod(&mut bufs, 1280 * 720 * 2, 1280 * 2));
        assert!(libspa::pod::Pod::from_bytes(&bufs).is_some());
    }

    #[test]
    fn test_vcam_blit_frame_rb_swap_and_mismatch() {
        // 2x2 BGRA -> RGBA swap
        let src = vec![
            10, 20, 30, 255, 40, 50, 60, 255, // row 0
            70, 80, 90, 255, 100, 110, 120, 255, // row 1
        ];
        let mut dst = vec![0u8; 16];
        blit_frame(&src, 2, 2, &mut dst, 2, 2, true);
        assert_eq!(&dst[0..4], &[30, 20, 10, 255]);
        assert_eq!(&dst[12..16], &[120, 110, 100, 255]);

        // Dimension mismatch (2x2 into 3x2): right column zero-filled cleanly, no shear
        let mut wide = vec![99u8; 3 * 2 * 4];
        blit_frame(&src, 2, 2, &mut wide, 3, 2, false);
        assert_eq!(&wide[0..4], &[10, 20, 30, 255]);
        assert_eq!(&wide[8..12], &[0, 0, 0, 0]);
        assert_eq!(&wide[12..16], &[70, 80, 90, 255]);
    }

    #[test]
    fn test_vcam_yuy2_and_i420_conversion() {
        // 2x2 pure white BGRA -> Y ~ 235, U ~ 128, V ~ 128
        let white = vec![255u8; 2 * 2 * 4];
        let mut yuy2 = vec![0u8; 2 * 2 * 2];
        blit_frame_formatted(&white, 2, 2, false, &mut yuy2, 2, 2, OutputPixelFormat::Yuy2);
        assert_eq!(yuy2[0], 235);
        assert_eq!(yuy2[1], 128);
        assert_eq!(yuy2[2], 235);
        assert_eq!(yuy2[3], 128);

        let mut i420 = vec![0u8; 2 * 2 * 3 / 2];
        blit_frame_formatted(&white, 2, 2, false, &mut i420, 2, 2, OutputPixelFormat::I420);
        assert_eq!(&i420[0..4], &[235, 235, 235, 235]);
        assert_eq!(i420[4], 128);
        assert_eq!(i420[5], 128);
    }
}
