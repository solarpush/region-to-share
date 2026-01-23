//! PipeWire stream handling for screen capture.
//!
//! Uses pipewire-rs to capture frames from the Portal screencast stream.

use region_capture::{CaptureError, Frame, FrameData, Result};
use region_core::{Rectangle, PixelFormat};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

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
    frame_rx: mpsc::UnboundedReceiver<PipeWireFrame>,
    stop_tx: Option<mpsc::UnboundedSender<()>>,
    sequence: u64,
    stream_width: u32,
    stream_height: u32,
}

impl PipeWireStream {
    /// Connect to a PipeWire node.
    pub async fn connect(node_id: u32) -> Result<Self> {
        let state = Arc::new(AtomicU64::new(1)); // Connecting
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let (stop_tx, mut stop_rx) = mpsc::unbounded_channel();
        
        let state_clone = state.clone();
        
        // Spawn PipeWire thread (PipeWire requires its own thread)
        thread::spawn(move || {
            if let Err(e) = run_pipewire_loop(node_id, frame_tx, &mut stop_rx, state_clone) {
                eprintln!("PipeWire error: {}", e);
            }
        });
        
        // Wait a bit for connection
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
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
        if state_val != 3 && state_val != 1 { // Not streaming or connecting
            return Err(CaptureError::CaptureFailed("Stream not active".to_string()));
        }

        // Try to get latest frame
        let mut latest_frame = None;
        while let Ok(frame) = self.frame_rx.try_recv() {
            latest_frame = Some(frame);
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

/// Run the PipeWire main loop (must run on its own thread).
fn run_pipewire_loop(
    node_id: u32,
    frame_tx: mpsc::UnboundedSender<PipeWireFrame>,
    stop_rx: &mut mpsc::UnboundedReceiver<()>,
    state: Arc<AtomicU64>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use pipewire as pw;
    
    // Initialize PipeWire
    pw::init();
    
    let mainloop = pw::main_loop::MainLoop::new(None)?;
    let context = pw::context::Context::new(&mainloop)?;
    let core = context.connect(None)?;
    
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
        .state_changed(move |_, _, old, new| {
            println!("PipeWire stream state: {:?} -> {:?}", old, new);
            let new_state = match new {
                pw::stream::StreamState::Paused => 2,
                pw::stream::StreamState::Streaming => 3,
                pw::stream::StreamState::Error(_) => 4,
                _ => 1,
            };
            state_clone.store(new_state, Ordering::Relaxed);
        })
        .process(move |stream, _| {
            if let Some(mut buffer) = stream.dequeue_buffer() {
                let datas = buffer.datas_mut();
                if !datas.is_empty() {
                    if let Some(data) = datas[0].data() {
                        // Get format info from stream
                        // For now assume BGRA and fixed size
                        let width = 1920u32; // TODO: get from format
                        let height = 1080u32;
                        
                        let pw_frame = PipeWireFrame {
                            width,
                            height,
                            data: Arc::new(data.to_vec()),
                            format: PixelFormat::BGRA8888,
                            timestamp: Instant::now(),
                        };
                        let _ = frame_tx_clone.send(pw_frame);
                    }
                }
            }
        })
        .register()?;
    
    // Supported formats
    let mut params = [
        pw::spa::pod::Pod::from_bytes(&[]).unwrap(), // Will be filled properly
    ];
    
    // Connect to the node
    stream.connect(
        pw::spa::utils::Direction::Input,
        Some(node_id),
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;
    
    state.store(3, Ordering::Relaxed); // Streaming
    
    // Run main loop
    mainloop.run();
    
    state.store(0, Ordering::Relaxed);
    Ok(())
}
