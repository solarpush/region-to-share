//! Crash-isolated live FFmpeg pipe recorder (`MKV`, Fragmented `MP4`, `WebM`, `GIF`, `MPEG-TS`/RTMP).

use crate::{
    EncoderCapabilities, RecorderBackend, RecorderError, RecordingJob, RecordingOutput,
    RecordingStats, RecordingSummary, Result, SelectedEncoder,
};
use async_trait::async_trait;
use crossbeam_channel::{self as channel, Sender, TrySendError};
use log::{debug, info, warn};
use region_capture::Frame;
use region_core::{AudioMode, ContainerFormat, RecordingConfig};
use region_platform::AppPaths;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/// Expand a filename template (e.g. `region-%Y%m%d-%H%M%S`) into a full output path.
pub fn expand_output_path(
    cfg: &RecordingConfig,
    preset_name: Option<&str>,
    override_container: Option<ContainerFormat>,
) -> PathBuf {
    let dir = if cfg.output_dir.trim().is_empty() {
        AppPaths::videos_dir()
    } else {
        PathBuf::from(&cfg.output_dir)
    };
    let mut name = region_core::format_local_timestamp(&cfg.filename_template);
    let safe_preset = preset_name
        .unwrap_or("region")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    name = name.replace("{preset}", &safe_preset);
    let container = override_container.unwrap_or(cfg.container);
    dir.join(format!("{name}.{}", container.extension()))
}

struct ActiveRecording {
    job: RecordingJob,
    encoder: SelectedEncoder,
    frame_tx: Option<Sender<Arc<Frame>>>,
    writer_thread: Option<thread::JoinHandle<()>>,
    child: Option<Child>,
    started_at: Instant,
    paused: Arc<AtomicBool>,
    frames_written: Arc<AtomicU64>,
    frames_dropped: Arc<AtomicU64>,
}

/// Live crash-safe recorder backed by an isolated `ffmpeg` sidecar process.
pub struct FfmpegPipeRecorder {
    caps: EncoderCapabilities,
    active: Option<ActiveRecording>,
}

impl FfmpegPipeRecorder {
    pub fn new() -> Self {
        Self::with_capabilities(EncoderCapabilities::probe())
    }

    /// Reuse already-probed [`EncoderCapabilities`] so starting a recording
    /// from the GUI does not re-run GPU encoder smoke-tests on every click.
    pub fn with_capabilities(caps: EncoderCapabilities) -> Self {
        Self { caps, active: None }
    }

    pub fn capabilities(&self) -> &EncoderCapabilities {
        &self.caps
    }

    fn build_ffmpeg_args(job: &RecordingJob, enc: &SelectedEncoder) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-y".into(),
        ];

        if enc.uses_vaapi {
            if let Some(dev) = &enc.vaapi_device {
                args.extend(["-vaapi_device".into(), dev.clone()]);
            }
        }

        // Raw video frames streamed in real time on stdin
        args.extend([
            "-f".into(),
            "rawvideo".into(),
            "-pixel_format".into(),
            job.input_format.ffmpeg_pix_fmt().into(),
            "-video_size".into(),
            format!("{}x{}", job.width, job.height),
            "-framerate".into(),
            job.fps.to_string(),
            "-thread_queue_size".into(),
            "64".into(),
            "-i".into(),
            "pipe:0".into(),
        ]);

        // Optional Linux PipeWire/PulseAudio inputs
        #[cfg(target_os = "linux")]
        match job.audio_mode {
            AudioMode::None => {}
            AudioMode::Desktop => {
                args.extend([
                    "-thread_queue_size".into(),
                    "128".into(),
                    "-f".into(),
                    "pulse".into(),
                    "-i".into(),
                    "@DEFAULT_MONITOR@".into(),
                ]);
            }
            AudioMode::Mic => {
                args.extend([
                    "-thread_queue_size".into(),
                    "128".into(),
                    "-f".into(),
                    "pulse".into(),
                    "-i".into(),
                    "default".into(),
                ]);
            }
            AudioMode::Both => {
                args.extend([
                    "-thread_queue_size".into(),
                    "128".into(),
                    "-f".into(),
                    "pulse".into(),
                    "-i".into(),
                    "@DEFAULT_MONITOR@".into(),
                    "-thread_queue_size".into(),
                    "128".into(),
                    "-f".into(),
                    "pulse".into(),
                    "-i".into(),
                    "default".into(),
                    "-filter_complex".into(),
                    "[1:a][2:a]amix=inputs=2:duration=longest[aout]".into(),
                    "-map".into(),
                    "0:v".into(),
                    "-map".into(),
                    "[aout]".into(),
                ]);
            }
        }

        // Video filter: guarantee even dimensions for YUV420p/VA-API + upload if VA-API
        let even_scale = "scale=trunc(iw/2)*2:trunc(ih/2)*2";
        if matches!(job.container, ContainerFormat::Gif) {
            args.extend(["-vf".into(), format!("fps={}", job.fps.min(20))]);
        } else if enc.uses_vaapi {
            args.extend([
                "-vf".into(),
                format!("{even_scale},format=nv12,hwupload"),
            ]);
        } else {
            args.extend([
                "-vf".into(),
                even_scale.into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
            ]);
        }

        // Video codec + low-latency live encoding tuning
        args.extend(["-c:v".into(), enc.ffmpeg_name.clone()]);
        match enc.ffmpeg_name.as_str() {
            "libx264" => {
                args.extend([
                    "-preset".into(),
                    "veryfast".into(),
                    "-tune".into(),
                    "zerolatency".into(),
                    "-crf".into(),
                    job.quality_crf.to_string(),
                ]);
            }
            "libopenh264" => {
                args.extend([
                    "-b:v".into(),
                    "8M".into(),
                    "-allow_skip_frames".into(),
                    "true".into(),
                ]);
            }
            "libx265" => {
                args.extend([
                    "-preset".into(),
                    "veryfast".into(),
                    "-crf".into(),
                    job.quality_crf.to_string(),
                ]);
            }
            "h264_vaapi" | "hevc_vaapi" | "vp9_vaapi" => {
                args.extend(["-qp".into(), job.quality_crf.clamp(16, 36).to_string()]);
            }
            "av1_vaapi" => {
                args.extend([
                    "-rc_mode".into(),
                    "CQP".into(),
                    "-global_quality".into(),
                    job.quality_crf.clamp(18, 42).to_string(),
                ]);
            }
            "h264_nvenc" | "hevc_nvenc" | "av1_nvenc" => {
                args.extend([
                    "-preset".into(),
                    "p4".into(),
                    "-tune".into(),
                    "ll".into(),
                    "-cq".into(),
                    job.quality_crf.to_string(),
                ]);
            }
            "h264_videotoolbox" | "hevc_videotoolbox" => {
                args.extend(["-b:v".into(), "8M".into(), "-realtime".into(), "true".into()]);
            }
            "libvpx-vp9" | "libvpx" => {
                args.extend([
                    "-deadline".into(),
                    "realtime".into(),
                    "-cpu-used".into(),
                    "6".into(),
                    "-crf".into(),
                    job.quality_crf.clamp(20, 42).to_string(),
                    "-b:v".into(),
                    "0".into(),
                ]);
            }
            "libsvtav1" => {
                args.extend([
                    "-preset".into(),
                    "10".into(),
                    "-crf".into(),
                    job.quality_crf.clamp(24, 45).to_string(),
                ]);
            }
            _ => {}
        }

        // Audio codec + `-shortest` so live PulseAudio input terminates cleanly as soon as video pipe closes
        if !matches!(job.audio_mode, AudioMode::None) && !matches!(job.container, ContainerFormat::Gif)
        {
            let audio_codec = match job.container {
                ContainerFormat::Webm => "libopus",
                _ => "aac",
            };
            args.extend([
                "-c:a".into(),
                audio_codec.into(),
                "-b:a".into(),
                "160k".into(),
                "-shortest".into(),
            ]);
        }

        // Live crash-safe muxing flags
        match job.container {
            ContainerFormat::Mp4 => {
                // Fragmented MP4 writes moof+mdat chunks live — readable even after SIGKILL/power loss!
                args.extend([
                    "-movflags".into(),
                    "+frag_keyframe+empty_moov+default_base_moof".into(),
                    "-frag_duration".into(),
                    "1000000".into(),
                ]);
            }
            ContainerFormat::Mkv => {
                // Flush clusters continuously for real-time live growth on disk
                args.extend(["-flush_packets".into(), "1".into()]);
            }
            _ => {
                args.extend(["-flush_packets".into(), "1".into()]);
            }
        }

        args.extend(["-f".into(), job.container.ffmpeg_format().into()]);

        match &job.output {
            RecordingOutput::File(path) => args.push(path.to_string_lossy().into_owned()),
            RecordingOutput::Stdout => args.push("pipe:1".into()),
            RecordingOutput::Url(url) => args.push(url.clone()),
        }

        args
    }

    fn current_file_size(output: &RecordingOutput) -> u64 {
        match output {
            RecordingOutput::File(p) => std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
            _ => 0,
        }
    }

    /// Optional lossless faststart remux for MP4 on clean stop so every browser/player seeks instantly.
    fn remux_faststart(path: &Path) {
        let tmp = path.with_extension("faststart.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                &path.to_string_lossy(),
                "-c",
                "copy",
                "-movflags",
                "+faststart",
                &tmp.to_string_lossy(),
            ])
            .status();

        if let Ok(st) = status {
            if st.success() {
                if let Err(e) = std::fs::rename(&tmp, path) {
                    warn!("[Recorder] Failed to replace faststart MP4: {}", e);
                    let _ = std::fs::remove_file(&tmp);
                } else {
                    debug!("[Recorder] Remuxed MP4 with +faststart");
                }
            } else {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

impl Default for FfmpegPipeRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RecorderBackend for FfmpegPipeRecorder {
    async fn start(&mut self, job: RecordingJob) -> Result<()> {
        if !self.caps.ffmpeg_available {
            return Err(RecorderError::FfmpegNotFound);
        }

        if let RecordingOutput::File(ref path) = job.output {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let enc = self
            .caps
            .select_encoder_for_size(
                job.container,
                job.codec,
                job.hw_accel,
                job.width,
                job.height,
            )
            .ok_or(RecorderError::NoEncoderAvailable(job.codec))?;

        let args = Self::build_ffmpeg_args(&job, &enc);
        info!(
            "[Recorder] Starting live recording -> {} (encoder={}, {}x{} @ {}fps, container={:?})",
            job.output.display_label(),
            enc.ffmpeg_name,
            job.width,
            job.height,
            job.fps,
            job.container
        );
        debug!("[Recorder] ffmpeg args: {:?}", args);

        let stdout_cfg = match job.output {
            RecordingOutput::Stdout => Stdio::inherit(),
            _ => Stdio::null(),
        };

        let mut child = Command::new("ffmpeg")
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(stdout_cfg)
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| RecorderError::InitFailed(format!("Failed to spawn ffmpeg: {e}")))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| RecorderError::InitFailed("Failed to open ffmpeg stdin".into()))?;

        let expected_bytes =
            (job.width as usize) * (job.height as usize) * job.input_format.bytes_per_pixel();
        let job_w = job.width;
        let job_h = job.height;
        let tick_dur = std::time::Duration::from_secs_f64(1.0 / job.fps.max(1) as f64);

        let (frame_tx, frame_rx) = channel::bounded::<Arc<Frame>>(24);
        let frames_written = Arc::new(AtomicU64::new(0));
        let frames_dropped = Arc::new(AtomicU64::new(0));
        let paused = Arc::new(AtomicBool::new(false));

        let fw = Arc::clone(&frames_written);
        let fd = Arc::clone(&frames_dropped);
        let paused_cl = Arc::clone(&paused);

        let writer_thread = thread::Builder::new()
            .name("region-recorder-writer".into())
            .spawn(move || {
                // Constant-Frame-Rate (CFR) wall-clock writer:
                // - Writes incoming frames immediately when dimensions match the FFmpeg rawvideo stream.
                // - When Wayland/PipeWire sends no new frames on a static screen, duplicates the last
                //   frame at exact `tick_dur` intervals so video duration stays 1:1 with wall-clock & audio!
                let mut last_valid_frame: Option<Arc<Frame>> = None;
                let mut next_cfr_tick: Option<Instant> = None;
                let mut warned_mismatch = false;

                loop {
                    let wait = match next_cfr_tick {
                        Some(t) if !paused_cl.load(Ordering::Relaxed) => {
                            t.saturating_duration_since(Instant::now())
                        }
                        _ => std::time::Duration::from_millis(50),
                    };

                    match frame_rx.recv_timeout(wait) {
                        Ok(frame) => {
                            if paused_cl.load(Ordering::Relaxed) {
                                next_cfr_tick = None;
                                continue;
                            }
                            if frame.width != job_w || frame.height != job_h {
                                if !warned_mismatch {
                                    warn!(
                                        "[Recorder] Ignoring frame with mismatched size {}x{} (expected {}x{})",
                                        frame.width, frame.height, job_w, job_h
                                    );
                                    warned_mismatch = true;
                                }
                                fd.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                            if let Some(bytes) = frame.data.as_buffer() {
                                if bytes.len() >= expected_bytes {
                                    if stdin.write_all(&bytes[..expected_bytes]).is_err() {
                                        warn!("[Recorder] ffmpeg stdin pipe closed");
                                        break;
                                    }
                                    fw.fetch_add(1, Ordering::Relaxed);
                                    last_valid_frame = Some(frame);
                                    next_cfr_tick = Some(Instant::now() + tick_dur);
                                }
                            }
                        }
                        Err(channel::RecvTimeoutError::Timeout) => {
                            if paused_cl.load(Ordering::Relaxed) {
                                next_cfr_tick = None;
                                continue;
                            }
                            if let Some(ref last) = last_valid_frame {
                                if let Some(bytes) = last.data.as_buffer() {
                                    if bytes.len() >= expected_bytes {
                                        if stdin.write_all(&bytes[..expected_bytes]).is_err() {
                                            warn!("[Recorder] ffmpeg stdin pipe closed");
                                            break;
                                        }
                                        fw.fetch_add(1, Ordering::Relaxed);
                                        let now = Instant::now();
                                        let target = next_cfr_tick.map(|t| t + tick_dur).unwrap_or(now + tick_dur);
                                        next_cfr_tick = Some(if target < now { now + tick_dur } else { target });
                                    }
                                }
                            }
                        }
                        Err(channel::RecvTimeoutError::Disconnected) => {
                            break;
                        }
                    }
                }
                let _ = stdin.flush();
            })
            .map_err(|e| RecorderError::InitFailed(format!("Writer thread failed: {e}")))?;

        self.active = Some(ActiveRecording {
            job,
            encoder: enc,
            frame_tx: Some(frame_tx),
            writer_thread: Some(writer_thread),
            child: Some(child),
            started_at: Instant::now(),
            paused,
            frames_written,
            frames_dropped,
        });

        Ok(())
    }

    fn push_frame(&self, frame: Arc<Frame>) -> Result<()> {
        let Some(ref rec) = self.active else {
            return Err(RecorderError::NotRunning);
        };
        if rec.paused.load(Ordering::Relaxed) {
            return Ok(());
        }
        if let Some(ref tx) = rec.frame_tx {
            match tx.try_send(frame) {
                Ok(()) => Ok(()),
                Err(TrySendError::Full(_)) => {
                    rec.frames_dropped.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
                Err(TrySendError::Disconnected(_)) => {
                    Err(RecorderError::WriteFailed("Encoder writer disconnected".into()))
                }
            }
        } else {
            Err(RecorderError::NotRunning)
        }
    }

    async fn pause(&mut self) -> Result<()> {
        let Some(ref rec) = self.active else {
            return Err(RecorderError::NotRunning);
        };
        rec.paused.store(true, Ordering::Relaxed);
        info!("[Recorder] Paused");
        Ok(())
    }

    async fn resume(&mut self) -> Result<()> {
        let Some(ref rec) = self.active else {
            return Err(RecorderError::NotRunning);
        };
        rec.paused.store(false, Ordering::Relaxed);
        info!("[Recorder] Resumed");
        Ok(())
    }

    async fn stop(&mut self) -> Result<RecordingSummary> {
        let mut rec = self.active.take().ok_or(RecorderError::NotRunning)?;
        let duration_secs = rec.started_at.elapsed().as_secs_f64();

        // Close channel -> writer flushes and closes stdin -> ffmpeg writes trailer cleanly
        drop(rec.frame_tx.take());
        if let Some(th) = rec.writer_thread.take() {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = th.join();
            })
            .await;
        }

        if let Some(mut child) = rec.child.take() {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = child.wait();
            })
            .await;
        }

        // Optional faststart remux for MP4 files
        let output_path = match &rec.job.output {
            RecordingOutput::File(p) => {
                if matches!(rec.job.container, ContainerFormat::Mp4)
                    && rec.job.remux_mp4_faststart
                    && p.exists()
                {
                    let p_clone = p.clone();
                    let _ = tokio::task::spawn_blocking(move || Self::remux_faststart(&p_clone))
                        .await;
                }
                Some(p.clone())
            }
            _ => None,
        };

        let final_bytes = Self::current_file_size(&rec.job.output);
        if final_bytes == 0 {
            if let Some(ref p) = output_path {
                let _ = std::fs::remove_file(p);
            }
        }
        let summary = RecordingSummary {
            output_label: rec.job.output.display_label(),
            output_path,
            duration_secs,
            frames_written: rec.frames_written.load(Ordering::Relaxed),
            frames_dropped: rec.frames_dropped.load(Ordering::Relaxed),
            final_bytes,
            encoder_name: rec.encoder.ffmpeg_name,
            container: rec.job.container,
        };

        info!(
            "[Recorder] Stopped -> {} ({:.1}s, {} frames, {} bytes)",
            summary.output_label,
            summary.duration_secs,
            summary.frames_written,
            summary.final_bytes
        );

        Ok(summary)
    }

    fn stats(&self) -> RecordingStats {
        match &self.active {
            Some(rec) => RecordingStats {
                active: true,
                paused: rec.paused.load(Ordering::Relaxed),
                elapsed_secs: rec.started_at.elapsed().as_secs_f64(),
                frames_written: rec.frames_written.load(Ordering::Relaxed),
                frames_dropped: rec.frames_dropped.load(Ordering::Relaxed),
                bytes_on_disk: Self::current_file_size(&rec.job.output),
                encoder_name: rec.encoder.ffmpeg_name.clone(),
                output_label: rec.job.output.display_label(),
            },
            None => RecordingStats::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use region_capture::FrameData;
    use region_core::{HardwareEncoder, PixelFormat, Rectangle, VideoCodec};

    #[tokio::test]
    async fn test_live_mkv_and_fmp4_recording_end_to_end() {
        let mut recorder = FfmpegPipeRecorder::new();
        if !recorder.capabilities().ffmpeg_available {
            eprintln!("Skipping ffmpeg live test (ffmpeg not installed)");
            return;
        }

        let dir = std::env::temp_dir().join(format!("region-to-share-rec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for container in [ContainerFormat::Mkv, ContainerFormat::Mp4] {
            let out_path = dir.join(format!("live_test.{}", container.extension()));
            let job = RecordingJob {
                output: RecordingOutput::File(out_path.clone()),
                width: 160,
                height: 120,
                fps: 30,
                input_format: PixelFormat::BGRA8888,
                container,
                codec: VideoCodec::H264,
                hw_accel: HardwareEncoder::Cpu,
                quality_crf: 23,
                audio_mode: AudioMode::None,
                remux_mp4_faststart: true,
                max_duration: None,
            };

            recorder.start(job).await.unwrap();
            for seq in 0..20u64 {
                let mut px = vec![0u8; 160 * 120 * 4];
                for chunk in px.chunks_exact_mut(4) {
                    chunk[0] = (seq * 10) as u8; // B
                    chunk[1] = 128; // G
                    chunk[2] = 220; // R
                    chunk[3] = 255;
                }
                let frame = Arc::new(Frame::new(
                    160,
                    120,
                    PixelFormat::BGRA8888,
                    FrameData::from_buffer(px),
                    Rectangle::new(0, 0, 160, 120),
                    seq,
                ));
                recorder.push_frame(frame).unwrap();
            }

            let summary = recorder.stop().await.unwrap();
            assert_eq!(summary.frames_written, 20);
            assert!(out_path.exists());
            assert!(
                summary.final_bytes > 500,
                "Expected valid encoded video for {:?}, got {} bytes",
                container,
                summary.final_bytes
            );
        }

        // Also test default Auto codec + Auto HW accel at 640x360 (exercises av1_vaapi on Fedora!)
        let auto_mkv = dir.join("live_auto.mkv");
        let job = RecordingJob {
            output: RecordingOutput::File(auto_mkv.clone()),
            width: 640,
            height: 360,
            fps: 30,
            input_format: PixelFormat::BGRA8888,
            container: ContainerFormat::Mkv,
            codec: VideoCodec::Auto,
            hw_accel: HardwareEncoder::Auto,
            quality_crf: 22,
            audio_mode: AudioMode::None,
            remux_mp4_faststart: true,
            max_duration: None,
        };
        recorder.start(job).await.unwrap();
        for seq in 0..15u64 {
            let mut px = vec![0u8; 640 * 360 * 4];
            for chunk in px.chunks_exact_mut(4) {
                chunk[0] = (seq * 15) as u8;
                chunk[1] = 90;
                chunk[2] = 200;
                chunk[3] = 255;
            }
            let frame = Arc::new(Frame::new(
                640,
                360,
                PixelFormat::BGRA8888,
                FrameData::from_buffer(px),
                Rectangle::new(0, 0, 640, 360),
                seq,
            ));
            recorder.push_frame(frame).unwrap();
        }
        let auto_summary = recorder.stop().await.unwrap();
        assert_eq!(auto_summary.frames_written, 15);
        assert!(
            auto_summary.final_bytes > 500,
            "Auto HW/CPU recording produced invalid file ({} bytes, encoder={})",
            auto_summary.final_bytes,
            auto_summary.encoder_name
        );

        // Verify CFR static-screen pacing: push 1 frame, stay static for 320ms at 30fps -> ~9-11 frames written
        let cfr_mkv = dir.join("live_cfr_static.mkv");
        let cfr_job = RecordingJob {
            output: RecordingOutput::File(cfr_mkv.clone()),
            width: 160,
            height: 120,
            fps: 30,
            input_format: PixelFormat::BGRA8888,
            container: ContainerFormat::Mkv,
            codec: VideoCodec::H264,
            hw_accel: HardwareEncoder::Cpu,
            quality_crf: 23,
            audio_mode: AudioMode::None,
            remux_mp4_faststart: false,
            max_duration: None,
        };
        recorder.start(cfr_job).await.unwrap();
        let static_frame = Arc::new(Frame::new(
            160,
            120,
            PixelFormat::BGRA8888,
            FrameData::from_buffer(vec![180u8; 160 * 120 * 4]),
            Rectangle::new(0, 0, 160, 120),
            1,
        ));
        recorder.push_frame(static_frame).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(320)).await;
        let cfr_summary = recorder.stop().await.unwrap();
        assert!(
            cfr_summary.frames_written >= 7,
            "Expected CFR duplication on static screen (>= 7 frames in 320ms @ 30fps), got {}",
            cfr_summary.frames_written
        );
        assert!(cfr_summary.final_bytes > 400);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
