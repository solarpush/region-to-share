//! High-performance PipeWire video stream receiver with in-callback cropping,
//! reusable buffer pooling, and dynamic format/stride handling.

#[cfg(feature = "pipewire")]
use region_capture::FrameData;
use region_capture::{CaptureError, Frame, Result};
use region_core::{PixelFormat, Rectangle, SharedBufferPool};
use std::sync::Arc;
use std::time::Instant;

/// PipeWire stream connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Disconnected,
    Connecting,
    Paused,
    Streaming,
    Error,
}

/// Frame delivered by PipeWire (already cropped in-callback when a crop region is active).
#[derive(Debug, Clone)]
pub struct PipeWireFrame {
    pub width: u32,
    pub height: u32,
    /// Full underlying stream resolution before cropping.
    pub source_width: u32,
    pub source_height: u32,
    pub data: Arc<Vec<u8>>,
    pub format: PixelFormat,
    pub timestamp: Instant,
    /// Whether `data` was already cropped to `applied_region` inside the callback.
    pub applied_region: Option<Rectangle>,
}

/// Stride-aware in-place row crop into a pooled buffer.
///
/// Copies ONLY the pixels inside `region`, respecting `src_stride` (which may
/// include alignment padding from PipeWire/compositor buffers) and avoiding a
/// full-screen intermediate allocation.
#[allow(dead_code)]
pub(crate) fn copy_cropped_strided(
    src: &[u8],
    src_width: u32,
    src_height: u32,
    src_stride: usize,
    bpp: usize,
    region: Option<Rectangle>,
    pool: &SharedBufferPool,
) -> (Vec<u8>, u32, u32, Option<Rectangle>) {
    let clamped = region.map(|r| r.clamp_to_bounds(src_width, src_height));

    let (rx, ry, rw, rh) = match clamped {
        Some(r)
            if !(r.x == 0 && r.y == 0 && r.width == src_width && r.height == src_height) =>
        {
            (r.x as usize, r.y as usize, r.width, r.height)
        }
        _ => {
            // Full frame: strip stride padding if present
            let tight_stride = src_width as usize * bpp;
            let dst_size = tight_stride * src_height as usize;
            let mut dst = pool.acquire_with_capacity(dst_size);
            if src_stride == tight_stride && src.len() >= dst_size {
                dst.extend_from_slice(&src[..dst_size]);
            } else {
                for y in 0..src_height as usize {
                    let row_start = y * src_stride;
                    let row_end = (row_start + tight_stride).min(src.len());
                    if row_start < src.len() {
                        dst.extend_from_slice(&src[row_start..row_end]);
                    }
                    let expected = (y + 1) * tight_stride;
                    if dst.len() < expected {
                        dst.resize(expected, 0);
                    }
                }
            }
            return (dst, src_width, src_height, None);
        }
    };

    let dst_stride = rw as usize * bpp;
    let dst_size = dst_stride * rh as usize;
    let mut dst = pool.acquire_with_capacity(dst_size);

    for y in 0..rh as usize {
        let src_y = (ry + y).min(src_height.saturating_sub(1) as usize);
        let row_start = src_y * src_stride + rx * bpp;
        let row_end = (row_start + dst_stride).min(src.len());
        if row_start < src.len() {
            dst.extend_from_slice(&src[row_start..row_end]);
        }
        let expected = (y + 1) * dst_stride;
        if dst.len() < expected {
            dst.resize(expected, 0);
        }
    }

    (dst, rw, rh, clamped)
}

#[cfg(feature = "pipewire")]
mod pw_impl {
    use super::*;
    use crossbeam_channel::{self as channel, Receiver, Sender};
    use log::{debug, error, info, trace};
    use parking_lot::RwLock;
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
    use std::thread;

    struct MainLoopQuitHandle {
        ptr: *mut pipewire::sys::pw_main_loop,
    }

    unsafe impl Send for MainLoopQuitHandle {}
    unsafe impl Sync for MainLoopQuitHandle {}

    impl MainLoopQuitHandle {
        fn quit(&self) {
            unsafe { pipewire::sys::pw_main_loop_quit(self.ptr) };
        }
    }

    pub struct PipeWireStream {
        node_id: u32,
        state: Arc<AtomicU64>,
        frame_rx: Receiver<PipeWireFrame>,
        quit_handle: Option<MainLoopQuitHandle>,
        thread_handle: Option<thread::JoinHandle<()>>,
        sequence: u64,
        stream_width: Arc<AtomicU32>,
        stream_height: Arc<AtomicU32>,
        active_crop: Arc<RwLock<Option<Rectangle>>>,
        cached_frame: Option<PipeWireFrame>,
    }

    impl PipeWireStream {
        pub async fn connect_with_fd(
            node_id: u32,
            pipewire_fd: i32,
            prefer_dmabuf: bool,
        ) -> Result<Self> {
            let state = Arc::new(AtomicU64::new(1));
            let (frame_tx, frame_rx) = channel::bounded(3);
            let (quit_tx, quit_rx) = channel::bounded::<MainLoopQuitHandle>(1);
            let stream_width = Arc::new(AtomicU32::new(0));
            let stream_height = Arc::new(AtomicU32::new(0));
            let active_crop: Arc<RwLock<Option<Rectangle>>> = Arc::new(RwLock::new(None));

            let state_clone = Arc::clone(&state);
            let sw_clone = Arc::clone(&stream_width);
            let sh_clone = Arc::clone(&stream_height);
            let crop_clone = Arc::clone(&active_crop);

            let thread_handle = thread::spawn(move || {
                if let Err(e) = run_pipewire_loop_with_fd(
                    node_id,
                    pipewire_fd,
                    prefer_dmabuf,
                    frame_tx,
                    quit_tx,
                    state_clone,
                    sw_clone,
                    sh_clone,
                    crop_clone,
                ) {
                    error!("[PipeWire] Thread error: {}", e);
                }
            });

            let timeout = std::time::Duration::from_secs(5);
            let start = Instant::now();
            loop {
                let current_state = state.load(Ordering::Relaxed);
                if current_state == 3 {
                    break;
                }
                if current_state == 4 {
                    return Err(CaptureError::InitFailed(
                        "PipeWire stream entered Error state".to_string(),
                    ));
                }
                if start.elapsed() > timeout {
                    return Err(CaptureError::InitFailed(
                        "Timeout waiting for PipeWire stream".to_string(),
                    ));
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(120)).await;
            let quit_handle = quit_rx.try_recv().ok();

            Ok(Self {
                node_id,
                state,
                frame_rx,
                quit_handle,
                thread_handle: Some(thread_handle),
                sequence: 0,
                stream_width,
                stream_height,
                active_crop,
                cached_frame: None,
            })
        }

        /// Update the crop rectangle applied inside the PipeWire callback.
        /// Pass `None` to receive full uncropped frames (e.g. screenshot mode).
        pub fn set_active_crop(&self, crop: Option<Rectangle>) {
            *self.active_crop.write() = crop;
        }

        /// Drop any cached or queued stale frames (e.g. uncropped screenshot frames)
        /// right after changing the active crop region.
        pub fn clear_cached_and_drain(&mut self) {
            self.cached_frame = None;
            while self.frame_rx.try_recv().is_ok() {}
        }

        /// Drop cached/queued cropped frames when switching back to full-screen screenshot mode
        /// (preserving a fresh uncropped frame from `probe_stream_size` if already full-screen).
        pub fn drop_cropped_cached_and_drain(&mut self) {
            if self
                .cached_frame
                .as_ref()
                .is_some_and(|f| f.applied_region.is_some())
            {
                self.cached_frame = None;
            }
            while self.frame_rx.try_recv().is_ok() {}
        }

        pub async fn capture_frame(&mut self, region: Rectangle) -> Result<Frame> {
            self.capture_frame_with_timeout(region, std::time::Duration::from_millis(100))
                .await
        }

        pub async fn capture_frame_with_timeout(
            &mut self,
            region: Rectangle,
            timeout: std::time::Duration,
        ) -> Result<Frame> {
            let state_val = self.state.load(Ordering::Relaxed);
            if state_val != 3 && state_val != 1 && state_val != 2 {
                return Err(CaptureError::CaptureFailed("Stream not active".to_string()));
            }

            let mut latest_frame: Option<PipeWireFrame> = self.cached_frame.take();
            while let Ok(frame) = self.frame_rx.try_recv() {
                latest_frame = Some(frame);
            }

            if latest_frame.is_none() {
                let rx = self.frame_rx.clone();
                match tokio::task::spawn_blocking(move || rx.recv_timeout(timeout)).await {
                    Ok(Ok(frame)) => latest_frame = Some(frame),
                    Ok(Err(channel::RecvTimeoutError::Timeout)) => {
                        return Err(CaptureError::CaptureFailed(
                            "Timeout waiting for PipeWire frame".to_string(),
                        ));
                    }
                    Ok(Err(channel::RecvTimeoutError::Disconnected)) => {
                        return Err(CaptureError::CaptureFailed("PipeWire stream closed".to_string()));
                    }
                    Err(_) => {
                        return Err(CaptureError::CaptureFailed(
                            "PipeWire recv task panicked".to_string(),
                        ));
                    }
                }
            }

            let pw_frame = latest_frame
                .ok_or_else(|| CaptureError::CaptureFailed("No frame available".to_string()))?;

            self.sequence += 1;

            // If callback already cropped to the exact requested region, zero extra copy!
            if pw_frame.applied_region == Some(region)
                || (pw_frame.applied_region.is_none()
                    && region.x == 0
                    && region.y == 0
                    && region.width == pw_frame.width
                    && region.height == pw_frame.height)
            {
                return Ok(Frame {
                    width: pw_frame.width,
                    height: pw_frame.height,
                    format: pw_frame.format,
                    data: FrameData::Buffer(pw_frame.data),
                    timestamp: pw_frame.timestamp,
                    sequence: self.sequence,
                    region: pw_frame.applied_region.unwrap_or(region),
                });
            }

            // Fallback crop (e.g. first frame arriving right after a crop update)
            let bpp = pw_frame.format.bytes_per_pixel();
            let pool = SharedBufferPool::new(2);
            let (buf, w, h, applied) = copy_cropped_strided(
                &pw_frame.data,
                pw_frame.width,
                pw_frame.height,
                pw_frame.width as usize * bpp,
                bpp,
                Some(region),
                &pool,
            );
            Ok(Frame {
                width: w,
                height: h,
                format: pw_frame.format,
                data: FrameData::Buffer(Arc::new(buf)),
                timestamp: pw_frame.timestamp,
                sequence: self.sequence,
                region: applied.unwrap_or(region),
            })
        }

        pub fn state(&self) -> StreamState {
            match self.state.load(Ordering::Relaxed) {
                0 => StreamState::Disconnected,
                1 => StreamState::Connecting,
                2 => StreamState::Paused,
                3 => StreamState::Streaming,
                _ => StreamState::Error,
            }
        }

        pub fn node_id(&self) -> u32 {
            self.node_id
        }

        pub fn stream_size(&self) -> (u32, u32) {
            let w = self.stream_width.load(Ordering::Relaxed);
            let h = self.stream_height.load(Ordering::Relaxed);
            (w, h)
        }

        pub async fn probe_stream_size(&mut self) -> (u32, u32) {
            let mut latest: Option<PipeWireFrame> = None;
            while let Ok(frame) = self.frame_rx.try_recv() {
                latest = Some(frame);
            }
            if latest.is_none() {
                let rx = self.frame_rx.clone();
                let timeout = std::time::Duration::from_secs(1);
                if let Ok(Ok(frame)) =
                    tokio::task::spawn_blocking(move || rx.recv_timeout(timeout)).await
                {
                    latest = Some(frame);
                }
            }
            if let Some(frame) = latest {
                self.stream_width
                    .store(frame.source_width, Ordering::Relaxed);
                self.stream_height
                    .store(frame.source_height, Ordering::Relaxed);
                debug!(
                    "[PipeWire] probe_stream_size: {}x{}",
                    frame.source_width, frame.source_height
                );
                self.cached_frame = Some(frame);
            }
            self.stream_size()
        }

        pub async fn disconnect(&mut self) -> Result<()> {
            if let Some(handle) = self.quit_handle.take() {
                handle.quit();
            }
            if let Some(th) = self.thread_handle.take() {
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = th.join();
                })
                .await;
            }
            self.state.store(0, Ordering::Relaxed);
            Ok(())
        }
    }

    impl Drop for PipeWireStream {
        fn drop(&mut self) {
            if let Some(handle) = self.quit_handle.take() {
                handle.quit();
            }
            if let Some(th) = self.thread_handle.take() {
                let _ = th.join();
            }
        }
    }

    fn build_shm_format_pod(buf: &mut Vec<u8>) -> bool {
        use libspa::param::format::{FormatProperties, MediaSubtype, MediaType};
        use libspa::param::video::VideoFormat;
        use libspa::param::ParamType;
        use libspa::pod::serialize::PodSerializer;
        use libspa::pod::{ChoiceValue, Object, Property, PropertyFlags, Value};
        use libspa::utils::{Choice, ChoiceEnum, ChoiceFlags, Id, SpaTypes};

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
                            default: Id(VideoFormat::BGRx.as_raw()),
                            alternatives: vec![
                                Id(VideoFormat::BGRA.as_raw()),
                                Id(VideoFormat::RGBx.as_raw()),
                                Id(VideoFormat::RGBA.as_raw()),
                            ],
                        },
                    ))),
                },
            ],
        };

        let mut cursor = std::io::Cursor::new(&mut *buf);
        PodSerializer::serialize(&mut cursor, &Value::Object(obj)).is_ok()
    }

    #[allow(clippy::too_many_arguments)]
    fn run_pipewire_loop_with_fd(
        node_id: u32,
        pipewire_fd: i32,
        _prefer_dmabuf: bool,
        frame_tx: Sender<PipeWireFrame>,
        quit_tx: Sender<MainLoopQuitHandle>,
        state: Arc<AtomicU64>,
        stream_width: Arc<AtomicU32>,
        stream_height: Arc<AtomicU32>,
        active_crop: Arc<RwLock<Option<Rectangle>>>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use pipewire as pw;
        use std::os::fd::FromRawFd;

        pw::init();

        let mainloop = pw::main_loop::MainLoopBox::new(None)?;
        let _ = quit_tx.send(MainLoopQuitHandle {
            ptr: mainloop.as_raw_ptr(),
        });

        let context = pw::context::ContextBox::new(&mainloop.loop_(), None)?;
        let core = if pipewire_fd >= 0 {
            debug!("PipeWire: connecting via portal fd {}", pipewire_fd);
            let owned_fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(pipewire_fd) };
            context.connect_fd(owned_fd, None)?
        } else {
            context.connect(None)?
        };

        let stream = pw::stream::StreamBox::new(
            &core,
            "region-to-share",
            pw::properties::properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Screen",
            },
        )?;

        let state_clone = Arc::clone(&state);
        let format_width = Arc::new(AtomicU32::new(0));
        let format_height = Arc::new(AtomicU32::new(0));
        let format_kind = Arc::new(AtomicU32::new(0)); // 0=BGRA, 1=RGBA
        let pool = SharedBufferPool::new(4);

        let fw_param = Arc::clone(&format_width);
        let fh_param = Arc::clone(&format_height);
        let fk_param = Arc::clone(&format_kind);
        let sw_param = Arc::clone(&stream_width);
        let sh_param = Arc::clone(&stream_height);

        let fw_proc = Arc::clone(&format_width);
        let fh_proc = Arc::clone(&format_height);
        let fk_proc = Arc::clone(&format_kind);

        let _listener = stream
            .add_local_listener_with_user_data(())
            .state_changed(move |_, _, old, new| {
                debug!("PipeWire stream state: {:?} -> {:?}", old, new);
                let new_state = match new {
                    pw::stream::StreamState::Paused => 2,
                    pw::stream::StreamState::Streaming => {
                        info!("PipeWire: Stream STREAMING");
                        3
                    }
                    pw::stream::StreamState::Error(e) => {
                        error!("PipeWire stream error: {}", e);
                        4
                    }
                    _ => 1,
                };
                state_clone.store(new_state, Ordering::Relaxed);
            })
            .param_changed(move |_, _, id, param| {
                if id != libspa::param::ParamType::Format.as_raw() {
                    return;
                }
                let Some(pod) = param else { return };
                let mut video_info = libspa::param::video::VideoInfoRaw::new();
                if video_info.parse(pod).is_ok() {
                    let size = video_info.size();
                    fw_param.store(size.width, Ordering::Relaxed);
                    fh_param.store(size.height, Ordering::Relaxed);
                    sw_param.store(size.width, Ordering::Relaxed);
                    sh_param.store(size.height, Ordering::Relaxed);
                    let is_rgba = matches!(
                        video_info.format(),
                        libspa::param::video::VideoFormat::RGBA
                            | libspa::param::video::VideoFormat::RGBx
                    );
                    fk_param.store(u32::from(is_rgba), Ordering::Relaxed);
                    info!(
                        "[PipeWire] Negotiated format: {:?} {}x{}",
                        video_info.format(),
                        size.width,
                        size.height
                    );
                }
            })
            .process(move |stream, _| {
                trace!("PipeWire process callback");
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let datas = buffer.datas_mut();
                if datas.is_empty() {
                    return;
                }

                let data_ref = &mut datas[0];
                let chunk = data_ref.chunk();
                let stride = chunk.stride() as usize;
                let data_size = chunk.size() as usize;
                let offset = chunk.offset() as usize;
                if stride == 0 || data_size == 0 {
                    return;
                }

                let bpp = 4usize;
                let negotiated_w = fw_proc.load(Ordering::Relaxed);
                let negotiated_h = fh_proc.load(Ordering::Relaxed);
                let pixel_format = if fk_proc.load(Ordering::Relaxed) == 1 {
                    PixelFormat::RGBA8888
                } else {
                    PixelFormat::BGRA8888
                };
                let crop_rect = *active_crop.read();

                // Read and crop directly inside the mapped PipeWire slice without
                // ever copying the uncropped 4K/5K frame!
                let cropped: Option<(Vec<u8>, u32, u32, u32, u32, Option<Rectangle>)> =
                    if let Some(data) = data_ref.data() {
                        let actual = data_size.min(data.len().saturating_sub(offset));
                        if actual == 0 {
                            None
                        } else {
                            let slice = &data[offset..offset + actual];
                            let src_w = if negotiated_w > 0 {
                                negotiated_w
                            } else {
                                (stride / bpp) as u32
                            };
                            let src_h = if negotiated_h > 0 {
                                negotiated_h
                            } else {
                                (actual / stride) as u32
                            };
                            if src_w == 0 || src_h == 0 {
                                None
                            } else {
                                let (buf, w, h, applied) = copy_cropped_strided(
                                    slice, src_w, src_h, stride, bpp, crop_rect, &pool,
                                );
                                Some((buf, w, h, src_w, src_h, applied))
                            }
                        }
                    } else {
                        // Manual mmap fallback (e.g. MemFd under Flatpak)
                        let raw = data_ref.as_raw();
                        let fd = raw.fd as i32;
                        let mapoffset = raw.mapoffset as i64;
                        let maxsize = raw.maxsize as usize;
                        if fd >= 0 && maxsize > 0 {
                            unsafe {
                                let ptr = libc::mmap(
                                    std::ptr::null_mut(),
                                    maxsize,
                                    libc::PROT_READ,
                                    libc::MAP_SHARED,
                                    fd,
                                    mapoffset,
                                );
                                if ptr != libc::MAP_FAILED {
                                    let actual = data_size.min(maxsize.saturating_sub(offset));
                                    let slice = std::slice::from_raw_parts(
                                        (ptr as *const u8).add(offset),
                                        actual,
                                    );
                                    let src_w = if negotiated_w > 0 {
                                        negotiated_w
                                    } else {
                                        (stride / bpp) as u32
                                    };
                                    let src_h = if negotiated_h > 0 {
                                        negotiated_h
                                    } else {
                                        (actual / stride) as u32
                                    };
                                    let res = if src_w > 0 && src_h > 0 {
                                        let (buf, w, h, applied) = copy_cropped_strided(
                                            slice, src_w, src_h, stride, bpp, crop_rect, &pool,
                                        );
                                        Some((buf, w, h, src_w, src_h, applied))
                                    } else {
                                        None
                                    };
                                    libc::munmap(ptr, maxsize);
                                    res
                                } else {
                                    None
                                }
                            }
                        } else {
                            None
                        }
                    };

                if let Some((buf, width, height, source_width, source_height, applied_region)) =
                    cropped
                {
                    stream_width.store(source_width, Ordering::Relaxed);
                    stream_height.store(source_height, Ordering::Relaxed);
                    let pw_frame = PipeWireFrame {
                        width,
                        height,
                        source_width,
                        source_height,
                        data: Arc::new(buf),
                        format: pixel_format,
                        timestamp: Instant::now(),
                        applied_region,
                    };
                    let _ = frame_tx.try_send(pw_frame);
                }
            })
            .register()?;

        let mut fmt_buf: Vec<u8> = Vec::with_capacity(512);
        let mut params_refs: Vec<&pw::spa::pod::Pod> = Vec::new();
        if build_shm_format_pod(&mut fmt_buf) {
            if let Some(pod) = libspa::pod::Pod::from_bytes(&fmt_buf) {
                params_refs.push(pod);
            }
        }

        stream.connect(
            pw::spa::utils::Direction::Input,
            Some(node_id),
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params_refs,
        )?;

        mainloop.run();
        state.store(0, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(not(feature = "pipewire"))]
mod pw_impl {
    use super::*;

    pub struct PipeWireStream;

    impl PipeWireStream {
        pub async fn connect_with_fd(
            _node_id: u32,
            _pipewire_fd: i32,
            _prefer_dmabuf: bool,
        ) -> Result<Self> {
            Err(CaptureError::NotAvailable(
                "Built without `pipewire` feature (install `pipewire-devel` and rebuild with default features)".into(),
            ))
        }

        pub fn set_active_crop(&self, _crop: Option<Rectangle>) {}

        pub fn clear_cached_and_drain(&mut self) {}

        pub fn drop_cropped_cached_and_drain(&mut self) {}

        pub async fn capture_frame(&mut self, _region: Rectangle) -> Result<Frame> {
            Err(CaptureError::NotAvailable("PipeWire feature disabled".into()))
        }

        pub async fn capture_frame_with_timeout(
            &mut self,
            _region: Rectangle,
            _timeout: std::time::Duration,
        ) -> Result<Frame> {
            Err(CaptureError::NotAvailable("PipeWire feature disabled".into()))
        }

        pub fn state(&self) -> StreamState {
            StreamState::Disconnected
        }

        pub fn node_id(&self) -> u32 {
            0
        }

        pub fn stream_size(&self) -> (u32, u32) {
            (0, 0)
        }

        pub async fn probe_stream_size(&mut self) -> (u32, u32) {
            (0, 0)
        }

        pub async fn disconnect(&mut self) -> Result<()> {
            Ok(())
        }
    }
}

pub use pw_impl::PipeWireStream;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_cropped_strided_with_padding() {
        let pool = SharedBufferPool::new(2);
        // 4x4 BGRA image with stride = 20 bytes (4 bytes padding per row)
        let stride = 20usize;
        let mut src = vec![0u8; stride * 4];
        for y in 0..4usize {
            for x in 0..4usize {
                let off = y * stride + x * 4;
                src[off] = (x + y * 10) as u8;
                src[off + 3] = 255;
            }
        }
        let region = Rectangle::new(1, 2, 2, 2);
        let (dst, w, h, applied) = copy_cropped_strided(&src, 4, 4, stride, 4, Some(region), &pool);
        assert_eq!((w, h), (2, 2));
        assert_eq!(applied, Some(region));
        assert_eq!(dst.len(), 2 * 2 * 4);
        assert_eq!(dst[0], 21); // x=1, y=2 -> 1 + 20 = 21
        assert_eq!(dst[4], 22); // x=2, y=2
        assert_eq!(dst[8], 31); // x=1, y=3
    }
}
