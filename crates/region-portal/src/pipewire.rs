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
    /// Connect to a PipeWire node using the portal's file descriptor.
    pub async fn connect_with_fd(node_id: u32, pipewire_fd: i32) -> Result<Self> {
        let state = Arc::new(AtomicU64::new(1)); // Connecting
        // Canal borné à 4 frames : le thread PipeWire fait try_send et abandonne
        // si plein. Sans ça, les frames (≈16 Mo chacune à 3840×1080) s'accumulent
        // dans un canal illimité → ~1 Go de RAM au bout de quelques secondes.
        // 4 frames ≈ 64 Mo max en tampon, marge suffisante pour absorber un pic.
        let (frame_tx, frame_rx) = channel::bounded(4);
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
        
        // If no frame yet, wait a bit for one to arrive (up to 2 seconds).
        // On utilise spawn_blocking pour ne pas bloquer le thread tokio avec
        // recv_timeout (appel synchrone crossbeam) → évite le plafond de fps
        // causé par la monopolisation du runtime async.
        if latest_frame.is_none() {
            let rx = self.frame_rx.clone();
            let timeout = std::time::Duration::from_secs(2);
            match tokio::task::spawn_blocking(move || rx.recv_timeout(timeout)).await {
                Ok(Ok(frame)) => latest_frame = Some(frame),
                Ok(Err(channel::RecvTimeoutError::Timeout)) => {
                    return Err(CaptureError::CaptureFailed("Timeout waiting for frame".to_string()));
                }
                Ok(Err(channel::RecvTimeoutError::Disconnected)) => {
                    return Err(CaptureError::CaptureFailed("Stream closed".to_string()));
                }
                Err(_join_err) => {
                    return Err(CaptureError::CaptureFailed("spawn_blocking panicked".to_string()));
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
            // Attend un frame jusqu'à 1 seconde (spawn_blocking pour ne pas
            // bloquer le thread tokio avec recv_timeout synchrone).
            let rx = self.frame_rx.clone();
            let timeout = std::time::Duration::from_secs(1);
            if let Ok(Ok(frame)) = tokio::task::spawn_blocking(move || rx.recv_timeout(timeout)).await {
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

/// Construit un pod SPA `EnumFormat` annonçant uniquement video/raw BGRx/BGRA
/// **sans** la propriété modifier. L'absence du modifier signal à PipeWire/au
/// compositeur que ce consumer ne supporte pas le DMA-BUF avec tiling → retombe
/// sur SHM (memfd linear). Fiable dans Flatpak où Mesa sandboxé ne peut pas
/// importer les buffers GPU tuilés. Le framerate est fixé par le Portal
/// Screencast lui-même — l'ajouter ici crée un conflit ("no more input formats").
fn build_shm_format_pod(buf: &mut Vec<u8>) -> bool {
    use libspa::pod::{Object, Property, PropertyFlags, Value, ChoiceValue};
    use libspa::utils::{Choice, ChoiceEnum, ChoiceFlags, Id, SpaTypes};
    use libspa::param::format::{FormatProperties, MediaType, MediaSubtype};
    use libspa::param::video::VideoFormat;
    use libspa::param::ParamType;
    use libspa::pod::serialize::PodSerializer;

    let obj = Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw() as u32,
        properties: vec![
            Property {
                key: FormatProperties::MediaType.as_raw() as u32,
                flags: PropertyFlags::empty(),
                value: Value::Id(Id(MediaType::Video.as_raw() as u32)),
            },
            Property {
                key: FormatProperties::MediaSubtype.as_raw() as u32,
                flags: PropertyFlags::empty(),
                value: Value::Id(Id(MediaSubtype::Raw.as_raw() as u32)),
            },
            Property {
                key: FormatProperties::VideoFormat.as_raw() as u32,
                flags: PropertyFlags::empty(),
                // Enum{ default=BGRx, alt=[BGRA] }
                value: Value::Choice(ChoiceValue::Id(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Enum {
                        default: Id(VideoFormat::BGRx.as_raw() as u32),
                        alternatives: vec![Id(VideoFormat::BGRA.as_raw() as u32)],
                    },
                ))),
            },
            // PAS de propriété modifier → pas de DMA-BUF, PipeWire utilisera SHM.
            // PAS de VideoFramerate : le Portal Screencast fixe lui-même le framerate
            // dans le graphe PipeWire ; l'ajouter ici crée un conflit de format
            // ("no more input formats") avec Mutter/GNOME.
        ],
    };

    let mut cursor = std::io::Cursor::new(&mut *buf);
    PodSerializer::serialize(&mut cursor, &Value::Object(obj)).is_ok()
}

fn run_pipewire_loop_with_fd(
    node_id: u32,
    pipewire_fd: i32,
    frame_tx: Sender<PipeWireFrame>,
    _stop_rx: &mut tokio_mpsc::UnboundedReceiver<()>,
    state: Arc<AtomicU64>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use pipewire as pw;
    use std::os::fd::FromRawFd;

    pw::init();

    let mainloop = pw::main_loop::MainLoopBox::new(None)?;
    let context = pw::context::ContextBox::new(&mainloop.loop_(), None)?;

    let core = if pipewire_fd >= 0 {
        debug!("PipeWire: Connecting with portal fd {}", pipewire_fd);
        // SAFETY: fd provient du portal ashpd, valide et owned par nous
        let owned_fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(pipewire_fd) };
        context.connect_fd(owned_fd, None)?
    } else {
        debug!("PipeWire: Connecting without portal fd (local)");
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

    let state_clone = state.clone();

    let _listener = stream
        .add_local_listener_with_user_data(())
        .state_changed(move |stream, _, old, new| {
            debug!("PipeWire stream state: {:?} -> {:?}", old, new);
            let new_state = match new {
                pw::stream::StreamState::Paused    => { debug!("PipeWire: Stream PAUSED");    2 },
                pw::stream::StreamState::Streaming => { info!("PipeWire: Stream STREAMING");  3 },
                pw::stream::StreamState::Error(e)  => { error!("PipeWire stream error: {}", e); 4 },
                _ => 1,
            };
            state_clone.store(new_state, Ordering::Relaxed);
            let _ = stream;
        })
        .process(move |stream, _| {
            trace!("PipeWire process callback");

            if let Some(mut buffer) = stream.dequeue_buffer() {
                let datas = buffer.datas_mut();
                if datas.is_empty() { return; }

                let data_ref = &mut datas[0];
                let chunk    = data_ref.chunk();
                let stride   = chunk.stride() as usize;
                let data_size = chunk.size() as usize;
                let offset   = chunk.offset() as usize;

                // MAP_BUFFERS garantit que data() est toujours Some pour SHM/MemFd.
                if let Some(data) = data_ref.data() {
                    let actual = data_size.min(data.len().saturating_sub(offset));
                    if stride == 0 || actual == 0 { return; }

                    let bpp    = 4usize;
                    let width  = (stride / bpp) as u32;
                    let height = (actual / stride) as u32;
                    if width == 0 || height == 0 { return; }

                    let pw_frame = PipeWireFrame {
                        width,
                        height,
                        data: Arc::new(data[offset..offset + actual].to_vec()),
                        format: PixelFormat::BGRA8888,
                        timestamp: Instant::now(),
                    };
                    // try_send : si le canal est plein (consommateur lent),
                    // on abandonne ce frame plutôt que d'accumuler en mémoire.
                    let _ = frame_tx.try_send(pw_frame);
                }
            }
        })
        .register()?;
    
    // Construire un pod EnumFormat SHM-only (sans propriété modifier).
    // L'absence du modifier signal que ce consumer ne gère pas le DMA-BUF tuilé
    // → PipeWire/le compositeur tombera sur SHM (memfd linéaire), qui fonctionne
    // de façon fiable dans le sandbox Flatpak.
    let mut fmt_buf: Vec<u8> = Vec::with_capacity(512);
    let pod_ok = build_shm_format_pod(&mut fmt_buf);
    let mut params: Vec<&pw::spa::pod::Pod> = if pod_ok {
        match libspa::pod::Pod::from_bytes(&fmt_buf) {
            Some(pod) => {
                debug!("PipeWire: connexion avec pod SHM-only ({} bytes)", fmt_buf.len());
                vec![pod]
            }
            None => {
                debug!("PipeWire: pod SHM invalide, connexion sans params");
                vec![]
            }
        }
    } else {
        debug!("PipeWire: build pod SHM échoué, connexion sans params");
        vec![]
    };

    // RT_PROCESS : dequeue buffers dans le process callback
    // MAP_BUFFERS : mapper automatiquement les buffers mémoire (MemFd/SHM)
    // Pas ALLOC_BUFFERS : en mode Portal le compositeur (producteur) alloue.
    stream.connect(
        pw::spa::utils::Direction::Input,
        Some(node_id),
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;
    debug!("PipeWire: stream connecté (SHM-only format pod)");
    
    // Run main loop - this will block and process events/frames
    mainloop.run();
    
    state.store(0, Ordering::Relaxed);
    Ok(())
}
