//! Live, crash-safe video and audio recording engine for Region to Share v3.
//!
//! Writes encoded packets continuously to disk (MKV, Fragmented MP4, WebM, GIF)
//! or live pipes (`stdout`, `rtmp://`, `srt://`) with constant low RAM usage
//! and automatic hardware encoder detection (VA-API, NVENC, VideoToolbox, CPU).

pub mod ffmpeg;
pub mod probe;

pub use ffmpeg::{expand_output_path, FfmpegPipeRecorder};
pub use probe::{EncoderCapabilities, SelectedEncoder};

use async_trait::async_trait;
use region_capture::Frame;
use region_core::{
    AudioMode, ContainerFormat, HardwareEncoder, PixelFormat, RecordingConfig, VideoCodec,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RecorderError {
    #[error("FFmpeg binary not found in PATH — install `ffmpeg` to enable live recording")]
    FfmpegNotFound,

    #[error("No compatible video encoder available for codec {0:?}")]
    NoEncoderAvailable(VideoCodec),

    #[error("Recorder initialization failed: {0}")]
    InitFailed(String),

    #[error("Recorder write failed: {0}")]
    WriteFailed(String),

    #[error("Recorder is not running")]
    NotRunning,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RecorderError>;

/// Output target for live recording.
#[derive(Debug, Clone)]
pub enum RecordingOutput {
    /// Write continuously to a file on disk.
    File(PathBuf),
    /// Stream raw container bytes directly to `stdout` (`-o -`).
    Stdout,
    /// Push live stream to a URL (`rtmp://...`, `srt://...`, `udp://...`).
    Url(String),
}

impl RecordingOutput {
    pub fn display_label(&self) -> String {
        match self {
            Self::File(p) => p.display().to_string(),
            Self::Stdout => "stdout (-)".to_string(),
            Self::Url(u) => u.clone(),
        }
    }
}

/// Complete specification of a live recording job.
#[derive(Debug, Clone)]
pub struct RecordingJob {
    pub output: RecordingOutput,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub input_format: PixelFormat,
    pub container: ContainerFormat,
    pub codec: VideoCodec,
    pub hw_accel: HardwareEncoder,
    pub quality_crf: u8,
    pub audio_mode: AudioMode,
    pub remux_mp4_faststart: bool,
    pub max_duration: Option<Duration>,
}

impl RecordingJob {
    pub fn from_config(
        cfg: &RecordingConfig,
        output: RecordingOutput,
        width: u32,
        height: u32,
        fps: u32,
        input_format: PixelFormat,
    ) -> Self {
        // Infer container from file extension if explicit file path has a known extension
        let container = match &output {
            RecordingOutput::File(p) => p
                .extension()
                .and_then(|e| e.to_str())
                .and_then(ContainerFormat::from_extension)
                .unwrap_or(cfg.container),
            RecordingOutput::Stdout => ContainerFormat::MpegTs,
            RecordingOutput::Url(_) => ContainerFormat::MpegTs,
        };

        Self {
            output,
            width,
            height,
            fps: fps.clamp(1, 240),
            input_format,
            container,
            codec: cfg.codec,
            hw_accel: cfg.hw_accel,
            quality_crf: cfg.quality_crf.min(51),
            audio_mode: cfg.audio_mode,
            remux_mp4_faststart: cfg.remux_mp4_faststart,
            max_duration: None,
        }
    }
}

/// Live telemetry snapshot updated continuously during recording.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordingStats {
    pub active: bool,
    pub paused: bool,
    pub elapsed_secs: f64,
    pub frames_written: u64,
    pub frames_dropped: u64,
    pub bytes_on_disk: u64,
    pub encoder_name: String,
    pub output_label: String,
}

/// Final summary returned when a recording cleanly stops.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSummary {
    pub output_label: String,
    pub output_path: Option<PathBuf>,
    pub duration_secs: f64,
    pub frames_written: u64,
    pub frames_dropped: u64,
    pub final_bytes: u64,
    pub encoder_name: String,
    pub container: ContainerFormat,
}

/// Extensible backend trait for live video recording (FFmpeg sidecar by default;
/// GStreamer zero-copy backend can implement this exact trait cleanly later).
#[async_trait]
pub trait RecorderBackend: Send + Sync {
    async fn start(&mut self, job: RecordingJob) -> Result<()>;
    fn push_frame(&self, frame: Arc<Frame>) -> Result<()>;
    async fn pause(&mut self) -> Result<()>;
    async fn resume(&mut self) -> Result<()>;
    async fn stop(&mut self) -> Result<RecordingSummary>;
    fn stats(&self) -> RecordingStats;
}
