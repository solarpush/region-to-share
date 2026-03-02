//! PipeWire stream handling for screen capture.
//!
//! Uses pipewire-rs to capture frames from the Portal screencast stream.

use region_capture::{CaptureError, Frame, FrameData, Result};
use region_core::{Rectangle, PixelFormat};
use std::sync::Arc;
use std::time::Instant;
use crossbeam_channel::{self as channel, Receiver, Sender};  // Thread-safe and Sync
use tokio::sync::mpsc as tokio_mpsc;  // Keep for stop signal
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use log::{debug, trace, error, info};

/// PipeWire stream state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Disconnected,
    Connecting,
    Paused,
    Streaming,
    Error,
}

/// Frame received from PipeWire.
#[derive(Debug, Clone)]
pub struct PipeWireFrame {
    pub width: u32,
    pub height: u32,
    pub data: Arc<Vec<u8>>,
    pub format: PixelFormat,
    pub timestamp: Instant,
}

/// PipeWire stream for receiving video frames.
pub struct PipeWireStream {
    node_id: u32,
    state: Arc<AtomicU64>, // 0=disconnected, 1=connecting, 2=paused, 3=streaming, 4=error
    frame_rx: Receiver<PipeWireFrame>,  // crossbeam-channel for thread-safe Sync
    stop_tx: Option<tokio_mpsc::UnboundedSender<()>>,
    sequence: u64,
    stream_width: u32,
    stream_height: u32,
}

impl PipeWireStream {
    /// Connect to a PipeWire node (without portal fd - deprecated).
    pub async fn connect(node_id: u32) -> Result<Self> {
        Self::connect_with_fd(node_id, -1).await
    }
    
    /// Connect to a PipeWire node using the portal's file descriptor.
    pub async fn connect_with_fd(node_id: u32, pipewire_fd: i32) -> Result<Self> {
        let state = Arc::new(AtomicU64::new(1)); // Connecting
        let (frame_tx, frame_rx) = channel::unbounded();  // crossbeam unbounded channel
        let (stop_tx, mut stop_rx) = tokio_mpsc::unbounded_channel();
        
        let state_clone = state.clone();
        
        // Spawn PipeWire thread (PipeWire requires its own thread)
        thread::spawn(move || {
            if let Err(e) = run_pipewire_loop_with_fd(node_id, pipewire_fd, frame_tx, &mut stop_rx, state_clone) {
                error!("[PipeWire] Thread error: {}", e);
            }
        });
        
        // Wait for stream to reach Streaming state (state == 3), with timeout
        let timeout = std::time::Duration::from_secs(5);
        let start = std::time::Instant::now();
        loop {
            let current_state = state.load(Ordering::Relaxed);
            if current_state == 3 {
                break;
            }
            if current_state == 4 {
                return Err(CaptureError::InitFailed("PipeWire stream error".to_string()));
            }
            if start.elapsed() > timeout {
                return Err(CaptureError::InitFailed("Timeout waiting for stream".to_string()));
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        
        // Give a bit more time for first frame to arrive
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        
        Ok(Self {
            node_id,
            state,
            frame_rx,
            stop_tx: Some(stop_tx),
            sequence: 0,
            stream_width: 1920,
            stream_height: 1080,
        })
    }

    /// Capture a frame from the stream.
    pub async fn capture_frame(&mut self, region: Rectangle) -> Result<Frame> {
        let state_val = self.state.load(Ordering::Relaxed);
        if state_val != 3 && state_val != 1 && state_val != 2 { // Not streaming, connecting, or paused
            return Err(CaptureError::CaptureFailed("Stream not active".to_string()));
        }

        // Try to get latest frame, with timeout for first frame
        let mut latest_frame = None;
        
        // First, drain any pending frames to get the latest (non-blocking)
        while let Ok(frame) = self.frame_rx.try_recv() {
            latest_frame = Some(frame);
        }
        
        // If no frame yet, wait a bit for one to arrive (up to 2 seconds)
        if latest_frame.is_none() {
            let timeout = std::time::Duration::from_secs(2);
            match self.frame_rx.recv_timeout(timeout) {
                Ok(frame) => latest_frame = Some(frame),
                Err(channel::RecvTimeoutError::Timeout) => {
                    return Err(CaptureError::CaptureFailed("Timeout waiting for frame".to_string()));
                }
                Err(channel::RecvTimeoutError::Disconnected) => {
                    return Err(CaptureError::CaptureFailed("Stream closed".to_string()));
                }
            }
        }
        
        if let Some(pw_frame) = latest_frame {
            self.sequence += 1;
            self.stream_width = pw_frame.width;
            self.stream_height = pw_frame.height;
            
            // Extract region from frame if needed
            let data = if region.x == 0 && region.y == 0 
                && region.width == pw_frame.width && region.height == pw_frame.height {
                pw_frame.data
            } else {
                extract_region(&pw_frame, &region)
            };
            
            Ok(Frame {
                width: region.width,
                height: region.height,
                format: pw_frame.format,
                data: FrameData::Buffer(data),
                timestamp: pw_frame.timestamp,
                sequence: self.sequence,
                region,
            })
        } else {
            // No new frame, return error or wait
            Err(CaptureError::CaptureFailed("No frame available".to_string()))
        }
    }

    /// Get the current stream state.
    pub fn state(&self) -> StreamState {
        match self.state.load(Ordering::Relaxed) {
            0 => StreamState::Disconnected,
            1 => StreamState::Connecting,
            2 => StreamState::Paused,
            3 => StreamState::Streaming,
            _ => StreamState::Error,
        }
    }

    /// Get the PipeWire node ID.
    pub fn node_id(&self) -> u32 {
        self.node_id
    }

    /// Get the stream size.
    pub fn stream_size(&self) -> (u32, u32) {
        (self.stream_width, self.stream_height)
    }

    /// Lit silencieusement un frame du channel pour initialiser les vraies
    /// dimensions du stream PipeWire. À appeler une fois après la connexion,
    /// avant le premier capture_frame(), pour que stream_size() soit fiable
    /// dès le cold-start (évite le ratio incorrect sur la première capture
    /// en cas de fractional scaling Wayland).
    pub async fn probe_stream_size(&mut self) -> (u32, u32) {
        // Draine d'abord les frames déjà en attente
        let mut latest: Option<PipeWireFrame> = None;
        while let Ok(frame) = self.frame_rx.try_recv() {
            latest = Some(frame);
        }
        if latest.is_none() {
            // Attend un frame jusqu'à 1 seconde
            let timeout = std::time::Duration::from_secs(1);
            if let Ok(frame) = self.frame_rx.recv_timeout(timeout) {
                latest = Some(frame);
            }
        }
        if let Some(frame) = latest {
            self.stream_width = frame.width;
            self.stream_height = frame.height;
            debug!("[PipeWire] probe_stream_size: {}x{}", frame.width, frame.height);
        }
        (self.stream_width, self.stream_height)
    }

    /// Disconnect from the stream.
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        self.state.store(0, Ordering::Relaxed);
        Ok(())
    }
}

/// Extract a region from a full frame.
fn extract_region(frame: &PipeWireFrame, region: &Rectangle) -> Arc<Vec<u8>> {
    let bpp = frame.format.bytes_per_pixel();
    let src_stride = frame.width as usize * bpp;
    let dst_stride = region.width as usize * bpp;
    let dst_size = region.height as usize * dst_stride;
    
    let mut dst = Vec::with_capacity(dst_size);
    
    for y in 0..region.height as usize {
        let src_y = (region.y as usize + y).min(frame.height as usize - 1);
        let src_offset = src_y * src_stride + region.x as usize * bpp;
        let src_end = (src_offset + dst_stride).min(frame.data.len());
        
        if src_offset < frame.data.len() {
            dst.extend_from_slice(&frame.data[src_offset..src_end]);
            // Pad if needed
            while dst.len() < (y + 1) * dst_stride {
                dst.push(0);
            }
        }
    }
    
    Arc::new(dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(width: u32, height: u32) -> PipeWireFrame {
        let bpp = 4usize;
        let size = (width as usize) * (height as usize) * bpp;
        let mut data = vec![0u8; size];
        for y in 0..height {
            for x in 0..width {
                let offset = ((y * width + x) as usize) * bpp;
                data[offset] = ((x + y * width) % 256) as u8;
                data[offset + 1] = 0;
                data[offset + 2] = 0;
                data[offset + 3] = 255;
            }
        }
        PipeWireFrame {
            width,
            height,
            data: Arc::new(data),
            format: PixelFormat::BGRA8888,
            timestamp: Instant::now(),
        }
    }

    #[test]
    fn extract_region_full_frame() {
        let frame = make_frame(100, 100);
        let region = Rectangle::new(0, 0, 100, 100);
        let result = extract_region(&frame, &region);
        assert_eq!(result.len(), frame.data.len());
    }

    #[test]
    fn extract_region_subset() {
        let frame = make_frame(100, 100);
        let region = Rectangle::new(10, 20, 30, 40);
        let result = extract_region(&frame, &region);
        let bpp = 4usize;
        let expected_size = 30 * 40 * bpp;
        assert_eq!(result.len(), expected_size);

        // Vérifie que le premier pixel extrait correspond à la bonne position source
        let src_offset = (20 * 100 + 10) * bpp;
        assert_eq!(result[0], frame.data[src_offset]);
    }

    #[test]
    fn extract_region_with_scaled_coordinates() {
        // Simule le fractional scaling : frame à 200x100, région en coordonnées scalées
        let frame = make_frame(200, 100);

        // Sans scaling : région (75, 50, 50, 25) dans un espace logique 150x75
        // Après scaling ×1.333 : (100, 66, 66, 33)
        let scaled_region = Rectangle::new(100, 66, 66, 33);
        let result = extract_region(&frame, &scaled_region);
        let bpp = 4usize;
        assert_eq!(result.len(), 66 * 33 * bpp);

        let src_offset = (66 * 200 + 100) * bpp;
        assert_eq!(result[0], frame.data[src_offset]);
    }
}

/// Run the PipeWire main loop with portal fd (must run on its own thread).
fn run_pipewire_loop_with_fd(
    node_id: u32,
    pipewire_fd: i32,
    frame_tx: Sender<PipeWireFrame>,
    _stop_rx: &mut tokio_mpsc::UnboundedReceiver<()>,
    state: Arc<AtomicU64>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use pipewire as pw;
    use std::os::fd::FromRawFd;
    use std::cell::RefCell;
    use std::rc::Rc;
    use crate::dmabuf_import::DmaBufImporter;
    use libspa::buffer::DataType;
    
    // Initialize PipeWire
    pw::init();
    
    // DmaBuf importer will be created lazily in the process callback thread
    // This is important because EGL contexts are thread-local
    let dmabuf_importer: Rc<RefCell<Option<DmaBufImporter>>> = Rc::new(RefCell::new(None));
    
    let mainloop = pw::main_loop::MainLoop::new(None)?;
    let context = pw::context::Context::new(&mainloop)?;
    
    // Connect using the portal's fd if provided, otherwise connect normally
    let core = if pipewire_fd >= 0 {
        debug!("PipeWire: Connecting with portal fd {}", pipewire_fd);
        // SAFETY: The fd comes from ashpd portal and is valid
        let owned_fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(pipewire_fd) };
        context.connect_fd(owned_fd, None)?
    } else {
        debug!("PipeWire: Connecting without portal fd (local)");
        context.connect(None)?
    };
    
    // Create stream
    let stream = pw::stream::Stream::new(
        &core,
        "region-to-share",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;
    
    let frame_tx_clone = frame_tx.clone();
    let state_clone = state.clone();
    
    // Stream listener for frame data
    let _listener = stream
        .add_local_listener_with_user_data(())
        .state_changed(move |stream, _, old, new| {
            debug!("PipeWire stream state: {:?} -> {:?}", old, new);
            let new_state = match new {
                pw::stream::StreamState::Paused => {
                    debug!("PipeWire: Stream PAUSED");
                    2
                },
                pw::stream::StreamState::Streaming => {
                    info!("PipeWire: Stream STREAMING");
                    3
                },
                pw::stream::StreamState::Error(e) => {
                    error!("PipeWire stream error: {}", e);
                    4
                },
                _ => 1,
            };
            state_clone.store(new_state, Ordering::Relaxed);
            let _ = stream; // Silence unused warning
        })
        .param_changed(move |_stream, _, id, pod| {
            trace!("PipeWire param_changed: id={}, has_pod={}", id, pod.is_some());
        })
        // Clone importer for process callback
        .process({
            let dmabuf_importer_clone = dmabuf_importer.clone();
            move |stream, _| {
            trace!("PipeWire process callback, state: {:?}", stream.state());
            
            if let Some(mut buffer) = stream.dequeue_buffer() {
                let datas = buffer.datas_mut();
                
                if !datas.is_empty() {
                    let data_ref = &mut datas[0];
                    let data_type = data_ref.type_();
                    
                    // Get raw spa_data to access fd - copy values to avoid borrow issues
                    let raw_data = data_ref.as_raw();
                    let fd = raw_data.fd;
                    let mapoffset = raw_data.mapoffset;
                    let maxsize = raw_data.maxsize;
                    
                    trace!("PipeWire buffer: fd={}, maxsize={}, type={:?}", fd, maxsize, data_type);
                    
                    // Get chunk info for actual data size
                    let chunk = data_ref.chunk();
                    let data_size = chunk.size() as usize;
                    let stride = chunk.stride() as usize;
                    let offset = chunk.offset() as usize;
                    
                    trace!("PipeWire chunk: size={}, stride={}, offset={}", data_size, stride, offset);
                    
                    // Try to get data - method depends on buffer type
                    // Returns (data, is_dmabuf) - DmaBuf gives RGBA, mmap gives BGRA
                    let frame_data: Option<(Vec<u8>, bool)> = match data_ref.data() {
                        Some(data) => {
                            let actual_size = data_size.min(data.len());
                            if actual_size > 0 {
                                trace!("PipeWire: Direct data {} bytes", actual_size);
                                Some((data[offset..offset + actual_size].to_vec(), false))
                            } else {
                                None
                            }
                        }
                        None => {
                            // Check if this is a DmaBuf
                            if data_type == DataType::DmaBuf {
                                trace!("PipeWire: DmaBuf detected, using EGL import");
                                // Calculate dimensions from stride
                                let bpp = 4; // BGRA/ARGB
                                let width = if stride > 0 { (stride / bpp) as u32 } else { 0 };
                                let height = if stride > 0 && data_size > 0 { 
                                    (data_size / stride) as u32 
                                } else { 
                                    0 
                                };
                                
                                trace!("PipeWire: DmaBuf dimensions {}x{}", width, height);
                                
                                if fd >= 0 && width > 0 && height > 0 {
                                    // Get or create the DmaBuf importer
                                    let mut importer_guard = dmabuf_importer_clone.borrow_mut();
                                    
                                    // Initialize importer if not yet done
                                    if importer_guard.is_none() {
                                        debug!("PipeWire: Creating DmaBufImporter");
                                        match DmaBufImporter::new() {
                                            Ok(imp) => {
                                                debug!("PipeWire: DmaBufImporter created successfully");
                                                *importer_guard = Some(imp);
                                            }
                                            Err(e) => {
                                                error!("Failed to create DmaBufImporter: {}", e);
                                            }
                                        }
                                    }
                                    
                                    if let Some(ref importer) = *importer_guard {
                                        // DRM_FORMAT_ARGB8888 = 0x34325241
                                        let fourcc = 0x34325241u32;
                                        
                                        trace!("PipeWire: Importing DmaBuf fd={} {}x{}", fd, width, height);
                                        match importer.import_dmabuf(
                                            fd as i32,
                                            width,
                                            height,
                                            stride as u32,
                                            offset as u32,
                                            fourcc,
                                        ) {
                                            Ok(pixels) => {
                                                trace!("PipeWire: DmaBuf import success, {} bytes", pixels.len());
                                                Some((pixels, true)) // DmaBuf gives RGBA
                                            }
                                            Err(e) => {
                                                error!("DmaBuf import failed: {}", e);
                                                None
                                            }
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else if fd >= 0 && maxsize > 0 {
                                // MemFd or other - try mmap
                                trace!("PipeWire: Trying mmap for fd={}", fd);
                                let map_size = maxsize as usize;
                                let map_offset = mapoffset as i64;
                                
                                match unsafe {
                                    libc::mmap(
                                        std::ptr::null_mut(),
                                        map_size,
                                        libc::PROT_READ,
                                        libc::MAP_SHARED,
                                        fd as i32,
                                        map_offset,
                                    )
                                } {
                                    ptr if ptr != libc::MAP_FAILED => {
                                        let slice = unsafe { 
                                            std::slice::from_raw_parts(
                                                (ptr as *const u8).add(offset), 
                                                data_size.min(map_size - offset)
                                            ) 
                                        };
                                        let result = slice.to_vec();
                                        unsafe { libc::munmap(ptr, map_size); }
                                        Some((result, false)) // mmap gives BGRA
                                    }
                                    _ => {
                                        None
                                    }
                                }
                            } else {
                                None
                            }
                        }
                    };
                    
                    if let Some((data, is_dmabuf)) = frame_data {
                        if stride > 0 && !data.is_empty() {
                            let bpp = 4;
                            let width = (stride / bpp) as u32;
                            let height = (data.len() / stride) as u32;
                            
                            // DmaBuf gives RGBA, mmap gives BGRA
                            let format = if is_dmabuf {
                                PixelFormat::RGBA8888
                            } else {
                                PixelFormat::BGRA8888
                            };
                            
                            if width > 0 && height > 0 {
                                let pw_frame = PipeWireFrame {
                                    width,
                                    height,
                                    data: Arc::new(data),
                                    format,
                                    timestamp: Instant::now(),
                                };
                                let _ = frame_tx_clone.send(pw_frame);
                            }
                        }
                    }
                }
            }
        }})
        .register()?;
    
    // Connect to the node without specific format params (let PipeWire negotiate)
    // For Portal streams, the format is already determined by the portal
    // NOTE: RT_PROCESS is required to dequeue buffers in the process callback
    // NOTE: ALLOC_BUFFERS makes PipeWire allocate buffers for us (non-DmaBuf)
    stream.connect(
        pw::spa::utils::Direction::Input,
        Some(node_id),
        pw::stream::StreamFlags::AUTOCONNECT 
            | pw::stream::StreamFlags::MAP_BUFFERS 
            | pw::stream::StreamFlags::RT_PROCESS
            | pw::stream::StreamFlags::ALLOC_BUFFERS,
        &mut [],  // Empty params = accept any format from portal
    )?;
    
    // Run main loop - this will block and process events/frames
    mainloop.run();
    
    state.store(0, Ordering::Relaxed);
    Ok(())
}
