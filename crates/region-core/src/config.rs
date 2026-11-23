//! Canonical v3 application configuration, presets, and recording profiles.

use crate::{
    error::{ConfigError, ConfigResult},
    geometry::{AspectRatio, Rectangle},
};
use serde::{Deserialize, Serialize};

/// Current config schema version (v3.0).
pub const CONFIG_SCHEMA_VERSION: u32 = 3;

/// Canonical application configuration persisted by `region-config`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Configuration schema version for transparent migrations.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    /// Capture backend, framerate, cursor, and region memory.
    #[serde(default)]
    pub capture: CaptureConfig,

    /// Wayland XDG Desktop Portal settings (including restore token & KDE direct mode).
    #[serde(default)]
    pub portal: PortalConfig,

    /// Live video/audio recording configuration.
    #[serde(default)]
    pub recording: RecordingConfig,

    /// Mirror display window & live overlay settings.
    #[serde(default)]
    pub display: DisplayConfig,

    /// Performance monitoring & optimization settings.
    #[serde(default)]
    pub performance: PerformanceConfig,

    /// UI preferences (theme, language, selection aids).
    #[serde(default)]
    pub ui: UiConfig,

    /// User-defined saved region presets (1-click share or record).
    #[serde(default)]
    pub presets: Vec<RegionPreset>,
}

fn default_schema_version() -> u32 {
    CONFIG_SCHEMA_VERSION
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            capture: CaptureConfig::default(),
            portal: PortalConfig::default(),
            recording: RecordingConfig::default(),
            display: DisplayConfig::default(),
            performance: PerformanceConfig::default(),
            ui: UiConfig::default(),
            presets: Vec::new(),
        }
    }
}

/// Capture backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    /// Preferred capture backend (`auto`, `x11`, `portal`, `pipewire`, `macos`, `windows`).
    #[serde(default)]
    pub backend: CaptureBackendKind,

    /// Target frame rate in FPS (1..=240).
    #[serde(default = "default_frame_rate")]
    pub frame_rate: u32,

    /// Include mouse cursor in captured frames.
    #[serde(default = "default_true")]
    pub show_cursor: bool,

    /// Remember the last selected region across launches.
    #[serde(default = "default_true")]
    pub remember_region: bool,

    /// Last captured region.
    #[serde(default)]
    pub last_region: Option<Rectangle>,

    /// Automatically start streaming using `last_region` on launch.
    #[serde(default)]
    pub auto_use_last_region: bool,
}

fn default_frame_rate() -> u32 {
    60
}

fn default_true() -> bool {
    true
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            backend: CaptureBackendKind::Auto,
            frame_rate: 60,
            show_cursor: true,
            remember_region: true,
            last_region: None,
            auto_use_last_region: false,
        }
    }
}

/// Capture backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureBackendKind {
    #[default]
    Auto,
    X11,
    Portal,
    PipeWire,
    MacOs,
    Windows,
}

/// Wayland XDG Desktop Portal configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalConfig {
    /// Persisted XDG ScreenCast restore token (avoids repeating the permission dialog).
    #[serde(default)]
    pub restore_token: Option<String>,

    /// How region selection interacts with the Wayland portal.
    #[serde(default)]
    pub selection_strategy: PortalSelectionStrategy,

    /// Enable DMA-BUF negotiation when outside restrictive sandboxes (falls back to SHM automatically).
    #[serde(default = "default_true")]
    pub prefer_dmabuf: bool,

    /// Publish captured region as a PipeWire virtual camera (`Video/Source`).
    #[serde(default)]
    pub enable_virtual_camera: bool,
}

impl Default for PortalConfig {
    fn default() -> Self {
        Self {
            restore_token: None,
            selection_strategy: PortalSelectionStrategy::Auto,
            prefer_dmabuf: true,
            enable_virtual_camera: false,
        }
    }
}

/// Strategy for Wayland portal region selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalSelectionStrategy {
    /// Automatically choose: on KDE Plasma 6+, offer direct KWin `stream_region` without double selection;
    /// on GNOME / wlroots, use our interactive overlay with persisted `restore_token`.
    #[default]
    Auto,
    /// Use the desktop portal's native stream directly without a second overlay crop
    /// (ideal on KDE Plasma 6 "Rectangular Region" or window streams).
    NativeDirect,
    /// Always capture a monitor stream and crop with Region to Share's interactive overlay
    /// (allows aspect-ratio locking, magnifier, live re-cropping, and saved presets).
    CustomOverlay,
}

/// Live recording configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    /// Output directory (empty = system default `~/Videos/RegionToShare`).
    #[serde(default)]
    pub output_dir: String,

    /// Filename template supporting `%Y`, `%m`, `%d`, `%H`, `%M`, `%S`, `{preset}`.
    #[serde(default = "default_filename_template")]
    pub filename_template: String,

    /// Output container format.
    #[serde(default)]
    pub container: ContainerFormat,

    /// Video codec preference.
    #[serde(default)]
    pub codec: VideoCodec,

    /// Hardware acceleration preference.
    #[serde(default)]
    pub hw_accel: HardwareEncoder,

    /// Quality CRF (lower = higher quality, typically 18..28).
    #[serde(default = "default_quality_crf")]
    pub quality_crf: u8,

    /// Audio capture mode.
    #[serde(default)]
    pub audio_mode: AudioMode,

    /// Remux fragmented MP4 with `+faststart` on clean stop for universal web/player seeking.
    #[serde(default = "default_true")]
    pub remux_mp4_faststart: bool,
}

fn default_filename_template() -> String {
    "region-%Y%m%d-%H%M%S".to_string()
}

fn default_quality_crf() -> u8 {
    20
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            output_dir: String::new(),
            filename_template: default_filename_template(),
            container: ContainerFormat::Mkv,
            codec: VideoCodec::Auto,
            hw_accel: HardwareEncoder::Auto,
            quality_crf: default_quality_crf(),
            audio_mode: AudioMode::None,
            remux_mp4_faststart: true,
        }
    }
}

/// Container format for live recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerFormat {
    /// Matroska — streamed cluster-by-cluster, inherently crash-safe.
    #[default]
    Mkv,
    /// Fragmented MP4 — live crash-safe (`+frag_keyframe+empty_moov+default_base_moof`).
    Mp4,
    /// WebM (VP9 / AV1) — web-friendly live container.
    Webm,
    /// Animated GIF.
    Gif,
    /// MPEG-TS — ideal for live stdout pipes and streaming.
    MpegTs,
}

impl ContainerFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Mkv => "mkv",
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
            Self::Gif => "gif",
            Self::MpegTs => "ts",
        }
    }

    pub const fn ffmpeg_format(self) -> &'static str {
        match self {
            Self::Mkv => "matroska",
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
            Self::Gif => "gif",
            Self::MpegTs => "mpegts",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "mkv" => Some(Self::Mkv),
            "mp4" | "m4v" => Some(Self::Mp4),
            "webm" => Some(Self::Webm),
            "gif" => Some(Self::Gif),
            "ts" | "mpegts" => Some(Self::MpegTs),
            _ => None,
        }
    }
}

/// Video codec preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    #[default]
    Auto,
    H264,
    Hevc,
    Av1,
    Vp9,
}

/// Hardware acceleration engine preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HardwareEncoder {
    #[default]
    Auto,
    Vaapi,
    Nvenc,
    VideoToolbox,
    MediaFoundation,
    Cpu,
}

/// Audio recording mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioMode {
    #[default]
    None,
    Desktop,
    Mic,
    Both,
}

/// Controls how streaming controls are presented so the mirror window stays clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudMode {
    /// Separate compact floating pill outside the shared window (mirror window = 100% pure pixels).
    #[default]
    DetachedPill,
    /// Subtle overlay that appears ONLY while hovering the mirror window and fades completely out otherwise.
    AutoHide,
    /// No overlay at all — controlled purely via KDE Plasmoid, GNOME Quick Settings, Tray, or CLI.
    TrayOnly,
}

/// What is rendered inside the mirror / stream window while sharing is paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseScreenMode {
    /// Branded pause card displaying the Region to Share logo and a custom message.
    #[default]
    Message,
    /// Seamlessly looped video file (decoded via FFmpeg) with Region to Share branding.
    Video,
}

/// Display window configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// Window opacity (0.1 ..= 1.0).
    #[serde(default = "default_opacity")]
    pub opacity: f32,

    /// Automatically send mirror window to background / minimize after capture starts.
    #[serde(default)]
    pub auto_background: bool,

    /// Show native OS window decorations on the mirror window.
    #[serde(default)]
    pub show_decorations: bool,

    /// Keep mirror window always on top.
    #[serde(default)]
    pub always_on_top: bool,

    /// How streaming controls are displayed.
    #[serde(default)]
    pub hud_mode: HudMode,

    /// Show a live click-through indicator border around the captured region.
    #[serde(default = "default_true")]
    pub show_capture_border: bool,

    /// Window title (visible in Meet/Discord/OBS window picker).
    #[serde(default = "default_window_title")]
    pub window_title: String,

    /// Pause screen mode rendered inside the stream window when sharing is paused.
    #[serde(default)]
    pub pause_mode: PauseScreenMode,

    /// Custom message shown when paused in `PauseScreenMode::Message` (empty = localized default).
    #[serde(default)]
    pub pause_message: String,

    /// Video file path looped continuously when paused in `PauseScreenMode::Video`.
    #[serde(default)]
    pub pause_video_path: String,
}

fn default_opacity() -> f32 {
    1.0
}

fn default_window_title() -> String {
    "Region to Share".to_string()
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            auto_background: false,
            show_decorations: false,
            always_on_top: false,
            hud_mode: HudMode::DetachedPill,
            show_capture_border: true,
            window_title: default_window_title(),
            pause_mode: PauseScreenMode::Message,
            pause_message: String::new(),
            pause_video_path: String::new(),
        }
    }
}

/// Performance monitoring and optimization configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    #[serde(default)]
    pub enable_profiling: bool,

    #[serde(default)]
    pub show_performance_overlay: bool,

    #[serde(default = "default_max_cpu")]
    pub max_cpu_usage: u32,

    #[serde(default = "default_true")]
    pub enable_gpu_acceleration: bool,

    #[serde(default = "default_true")]
    pub adaptive_quality: bool,
}

fn default_max_cpu() -> u32 {
    50
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enable_profiling: false,
            show_performance_overlay: false,
            max_cpu_usage: 50,
            enable_gpu_acceleration: true,
            adaptive_quality: true,
        }
    }
}

/// UI preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// UI theme (`auto`, `light`, `dark`).
    #[serde(default)]
    pub theme: UiTheme,

    /// Language override (`""` = auto-detect from system locale, `"fr"`, `"en"`).
    #[serde(default)]
    pub language: String,

    /// UI scale factor (`1.0` = 100%).
    #[serde(default = "default_opacity")]
    pub scale_factor: f32,

    /// Show rule-of-thirds / alignment grid during selection.
    #[serde(default = "default_true")]
    pub show_selection_grid: bool,

    /// Show pixel magnifier loupe during selection.
    #[serde(default = "default_true")]
    pub show_magnifier: bool,

    /// Grid snap spacing in pixels.
    #[serde(default = "default_grid_size")]
    pub grid_size: u32,

    /// Default aspect ratio lock in region selector.
    #[serde(default)]
    pub default_aspect_ratio: AspectRatio,
}

fn default_grid_size() -> u32 {
    16
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: UiTheme::Auto,
            language: String::new(),
            scale_factor: 1.0,
            show_selection_grid: true,
            show_magnifier: true,
            grid_size: 16,
            default_aspect_ratio: AspectRatio::Free,
        }
    }
}

/// UI theme selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiTheme {
    #[default]
    Auto,
    Light,
    Dark,
}

/// Action executed when launching a saved preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetAction {
    #[default]
    Share,
    Record,
    ShareAndRecord,
    VirtualCamera,
}

/// A saved 1-click region preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionPreset {
    pub id: String,
    pub name: String,
    pub region: Rectangle,
    #[serde(default)]
    pub aspect_ratio: AspectRatio,
    #[serde(default)]
    pub default_action: PresetAction,
    #[serde(default = "default_frame_rate")]
    pub fps: u32,
    #[serde(default = "default_true")]
    pub show_cursor: bool,
}

/// Canonical runtime session state reported over IPC / D-Bus / CLI status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStateKind {
    #[default]
    Idle,
    Selecting,
    Sharing,
    Recording,
    SharingAndRecording,
    Paused,
}

impl SessionStateKind {
    pub const fn as_dbus_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Selecting => "selecting",
            Self::Sharing => "sharing",
            Self::Recording => "recording",
            Self::SharingAndRecording => "sharing+recording",
            Self::Paused => "paused",
        }
    }

    pub fn from_dbus_str(s: &str) -> Self {
        match s {
            "selecting" => Self::Selecting,
            "sharing" => Self::Sharing,
            "recording" => Self::Recording,
            "sharing+recording" => Self::SharingAndRecording,
            "paused" => Self::Paused,
            _ => Self::Idle,
        }
    }
}

impl AppConfig {
    /// Validate configuration invariants.
    pub fn validate(&self) -> ConfigResult<()> {
        if self.capture.frame_rate == 0 || self.capture.frame_rate > 240 {
            return Err(ConfigError::InvalidValue {
                key: "capture.frame_rate".to_string(),
                message: "must be between 1 and 240".to_string(),
            });
        }
        if !(0.1..=1.0).contains(&self.display.opacity) {
            return Err(ConfigError::InvalidValue {
                key: "display.opacity".to_string(),
                message: "must be between 0.1 and 1.0".to_string(),
            });
        }
        if self.ui.scale_factor <= 0.0 || self.ui.scale_factor > 4.0 {
            return Err(ConfigError::InvalidValue {
                key: "ui.scale_factor".to_string(),
                message: "must be between 0.1 and 4.0".to_string(),
            });
        }
        if self.recording.quality_crf > 51 {
            return Err(ConfigError::InvalidValue {
                key: "recording.quality_crf".to_string(),
                message: "must be between 0 and 51".to_string(),
            });
        }
        Ok(())
    }

    /// Find a preset by id or case-insensitive name.
    pub fn find_preset(&self, query: &str) -> Option<&RegionPreset> {
        let q = query.trim();
        self.presets
            .iter()
            .find(|p| p.id == q || p.name.eq_ignore_ascii_case(q))
    }

    /// Add or replace a preset by name/id.
    pub fn upsert_preset(&mut self, preset: RegionPreset) {
        if let Some(existing) = self
            .presets
            .iter_mut()
            .find(|p| p.id == preset.id || p.name.eq_ignore_ascii_case(&preset.name))
        {
            *existing = preset;
        } else {
            self.presets.push(preset);
        }
    }

    /// Remove a preset by id or case-insensitive name. Returns true if removed.
    pub fn remove_preset(&mut self, query: &str) -> bool {
        let before = self.presets.len();
        self.presets
            .retain(|p| p.id != query && !p.name.eq_ignore_ascii_case(query));
        self.presets.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(cfg.capture.frame_rate, 60);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_preset_crud() {
        let mut cfg = AppConfig::default();
        cfg.upsert_preset(RegionPreset {
            id: "pres-1".into(),
            name: "Presentation 16:9".into(),
            region: Rectangle::new(0, 0, 1920, 1080),
            aspect_ratio: AspectRatio::Ratio16x9,
            default_action: PresetAction::Share,
            fps: 60,
            show_cursor: true,
        });
        assert!(cfg.find_preset("presentation 16:9").is_some());
        assert!(cfg.remove_preset("pres-1"));
        assert!(cfg.presets.is_empty());
    }

    #[test]
    fn test_validation_bounds() {
        let mut cfg = AppConfig::default();
        cfg.recording.quality_crf = 60;
        assert!(cfg.validate().is_err());

        cfg.recording.quality_crf = 20;
        cfg.capture.frame_rate = 0;
        assert!(cfg.validate().is_err());
    }
}
