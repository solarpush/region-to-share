//! Configuration types and defaults.

use crate::{error::ConfigResult, geometry::Rectangle};
use serde::{Deserialize, Serialize};

/// Main application configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Capture configuration.
    #[serde(default)]
    pub capture: CaptureConfig,

    /// Display window configuration.
    #[serde(default)]
    pub display: DisplayConfig,

    /// Performance configuration.
    #[serde(default)]
    pub performance: PerformanceConfig,

    /// UI preferences.
    #[serde(default)]
    pub ui: UiConfig,
}

/// Capture backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    /// Preferred capture backend (auto, x11, portal, pipewire).
    pub backend: CaptureBackend,

    /// Target frame rate in FPS.
    pub frame_rate: u32,

    /// Show cursor in captured frames.
    pub show_cursor: bool,

    /// Remember the last selected region.
    pub remember_region: bool,

    /// Last captured region (if remember_region is true).
    pub last_region: Option<Rectangle>,

    /// Automatically use the last region without asking.
    pub auto_use_last_region: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            backend: CaptureBackend::Auto,
            frame_rate: 30,
            show_cursor: true,
            remember_region: false,
            last_region: None,
            auto_use_last_region: false,
        }
    }
}

/// Capture backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureBackend {
    /// Automatically detect the best backend.
    Auto,
    /// Force X11 backend (XShm).
    X11,
    /// Force Portal backend (Wayland).
    Portal,
    /// Force PipeWire backend (direct connection).
    PipeWire,
}

/// Display window configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// Window opacity (0.0 - 1.0).
    pub opacity: f32,

    /// Automatically send window to background after starting capture.
    pub auto_background: bool,

    /// Show window decorations.
    pub show_decorations: bool,

    /// Always on top.
    pub always_on_top: bool,

    /// Window title.
    pub window_title: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            auto_background: false,
            show_decorations: true,
            always_on_top: false,
            window_title: "Region to Share".to_string(),
        }
    }
}

/// Performance monitoring and optimization configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable performance profiling.
    pub enable_profiling: bool,

    /// Show performance overlay in display window.
    pub show_performance_overlay: bool,

    /// Target maximum CPU usage percentage.
    pub max_cpu_usage: u32,

    /// Enable GPU acceleration (DMA-BUF, zero-copy).
    pub enable_gpu_acceleration: bool,

    /// Skip frames if CPU is overloaded.
    pub adaptive_quality: bool,
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
    /// UI theme (auto, light, dark).
    pub theme: UiTheme,

    /// UI scale factor (1.0 = 100%).
    pub scale_factor: f32,

    /// Show selection grid during region selection.
    pub show_selection_grid: bool,

    /// Grid size in pixels.
    pub grid_size: u32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: UiTheme::Auto,
            scale_factor: 1.0,
            show_selection_grid: false,
            grid_size: 10,
        }
    }
}

/// UI theme selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiTheme {
    /// Follow system theme.
    Auto,
    /// Light theme.
    Light,
    /// Dark theme.
    Dark,
}

impl Config {
    /// Validate the configuration.
    pub fn validate(&self) -> ConfigResult<()> {
        // Validate frame rate
        if self.capture.frame_rate == 0 || self.capture.frame_rate > 240 {
            return Err(crate::error::ConfigError::InvalidValue {
                key: "capture.frame_rate".to_string(),
                message: "must be between 1 and 240".to_string(),
            });
        }

        // Validate opacity
        if !(0.0..=1.0).contains(&self.display.opacity) {
            return Err(crate::error::ConfigError::InvalidValue {
                key: "display.opacity".to_string(),
                message: "must be between 0.0 and 1.0".to_string(),
            });
        }

        // Validate scale factor
        if self.ui.scale_factor <= 0.0 || self.ui.scale_factor > 4.0 {
            return Err(crate::error::ConfigError::InvalidValue {
                key: "ui.scale_factor".to_string(),
                message: "must be between 0.1 and 4.0".to_string(),
            });
        }

        Ok(())
    }

    /// Merge this config with overrides from another config.
    /// Non-default values from `other` will override values in `self`.
    pub fn merge(&mut self, other: &Config) {
        // This is a simple merge - in a real implementation you might want
        // to check if values are actually different from defaults
        self.capture.frame_rate = other.capture.frame_rate;
        self.display.opacity = other.display.opacity;
        // ... merge other fields as needed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.capture.frame_rate, 30);
        assert_eq!(config.display.opacity, 1.0);
        assert!(config.capture.show_cursor);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());

        // Invalid frame rate
        config.capture.frame_rate = 0;
        assert!(config.validate().is_err());

        config.capture.frame_rate = 30;
        assert!(config.validate().is_ok());

        // Invalid opacity
        config.display.opacity = 1.5;
        assert!(config.validate().is_err());

        config.display.opacity = 0.8;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_capture_backend_serialization() {
        let backend = CaptureBackend::Auto;
        let json = serde_json::to_string(&backend).unwrap();
        assert_eq!(json, r#""auto""#);

        let backend: CaptureBackend = serde_json::from_str(r#""x11""#).unwrap();
        assert_eq!(backend, CaptureBackend::X11);
    }

    #[test]
    fn test_ui_theme_serialization() {
        let theme = UiTheme::Dark;
        let json = serde_json::to_string(&theme).unwrap();
        assert_eq!(json, r#""dark""#);

        let theme: UiTheme = serde_json::from_str(r#""light""#).unwrap();
        assert_eq!(theme, UiTheme::Light);
    }
}
