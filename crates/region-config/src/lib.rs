//! Persistent configuration store for Region to Share v3.
//!
//! Uses `region_core::AppConfig` as the single source of truth, performs atomic
//! disk writes (`tmp` + `rename`), and automatically migrates legacy v2
//! `settings.json` files without losing user preferences.

use log::{debug, info, warn};
use region_core::{
    AppConfig, AspectRatio, CaptureBackendKind, PresetAction, Rectangle, RegionPreset,
    CONFIG_SCHEMA_VERSION,
};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Legacy v2 `settings.json` layout kept solely for transparent upgrade.
#[derive(Debug, Deserialize)]
struct LegacyV2Settings {
    #[serde(default = "default_v2_fps")]
    frame_rate: u32,
    #[serde(default)]
    capture_mode: String,
    #[serde(default)]
    show_performance: bool,
    #[serde(default)]
    auto_send_to_background: bool,
    #[serde(default)]
    remember_last_region: bool,
    #[serde(default)]
    auto_use_specific_region: bool,
    #[serde(default)]
    last_region: Option<LegacyV2Region>,
    #[serde(default)]
    language: String,
}

fn default_v2_fps() -> u32 {
    60
}

#[derive(Debug, Deserialize)]
struct LegacyV2Region {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

/// Persistent configuration manager.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    config_dir: PathBuf,
    config_file: PathBuf,
    pub config: AppConfig,
}

impl ConfigStore {
    /// Load configuration from the standard XDG / OS config directory.
    pub fn load_default() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
            })
            .join("region-to-share");
        Self::load_from_dir(config_dir)
    }

    /// Load configuration from an explicit directory (useful for tests and portable mode).
    pub fn load_from_dir(config_dir: PathBuf) -> Self {
        let config_file = config_dir.join("settings.json");
        let config = Self::read_or_migrate(&config_file);
        Self {
            config_dir,
            config_file,
            config,
        }
    }

    /// Backwards-compatible constructor alias.
    pub fn new() -> Self {
        Self::load_default()
    }

    pub fn config_path(&self) -> &Path {
        &self.config_file
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    fn read_or_migrate(config_file: &Path) -> AppConfig {
        if !config_file.exists() {
            return AppConfig::default();
        }

        let content = match fs::read_to_string(config_file) {
            Ok(c) => c,
            Err(e) => {
                warn!("[Config] Cannot read {:?}: {}", config_file, e);
                return AppConfig::default();
            }
        };

        // Check whether this file already carries schema_version >= 3
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
            let schema_ver = value
                .get("schema_version")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            if schema_ver >= CONFIG_SCHEMA_VERSION as u64 {
                match serde_json::from_value::<AppConfig>(value) {
                    Ok(mut cfg) => {
                        if cfg.validate().is_err() {
                            warn!("[Config] Loaded v3 config had out-of-range values; clamping");
                            cfg.capture.frame_rate = cfg.capture.frame_rate.clamp(1, 240);
                            cfg.display.opacity = cfg.display.opacity.clamp(0.1, 1.0);
                            cfg.ui.scale_factor = cfg.ui.scale_factor.clamp(0.5, 4.0);
                            cfg.recording.quality_crf = cfg.recording.quality_crf.min(51);
                        }
                        debug!("[Config] Loaded v3 settings from {:?}", config_file);
                        return cfg;
                    }
                    Err(e) => {
                        warn!("[Config] Failed to parse v3 config: {}", e);
                        return AppConfig::default();
                    }
                }
            }

            // Legacy v2 migration
            if let Ok(v2) = serde_json::from_value::<LegacyV2Settings>(value) {
                info!(
                    "[Config] Migrating legacy v2 settings from {:?} to v3",
                    config_file
                );
                let mut cfg = AppConfig::default();
                cfg.capture.frame_rate = v2.frame_rate.clamp(1, 240);
                cfg.capture.backend = match v2.capture_mode.to_lowercase().as_str() {
                    "x11" => CaptureBackendKind::X11,
                    "portal" => CaptureBackendKind::Portal,
                    "pipewire" => CaptureBackendKind::PipeWire,
                    _ => CaptureBackendKind::Auto,
                };
                cfg.performance.show_performance_overlay = v2.show_performance;
                cfg.display.auto_background = v2.auto_send_to_background;
                cfg.capture.remember_region = v2.remember_last_region;
                cfg.capture.auto_use_last_region = v2.auto_use_specific_region;
                cfg.capture.last_region = v2.last_region.and_then(|r| {
                    if r.width > 0 && r.height > 0 {
                        Some(Rectangle::new(r.x, r.y, r.width, r.height))
                    } else {
                        None
                    }
                });
                cfg.ui.language = v2.language;
                return cfg;
            }
        }

        warn!("[Config] Unrecognized config file format; using defaults");
        AppConfig::default()
    }

    /// Persist configuration atomically to disk.
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        fs::create_dir_all(&self.config_dir)?;
        let content = serde_json::to_string_pretty(&self.config)?;
        let tmp_path = self.config_dir.join("settings.json.tmp");
        fs::write(&tmp_path, content)?;
        fs::rename(&tmp_path, &self.config_file)?;
        debug!("[Config] Saved v3 settings to {:?}", self.config_file);
        Ok(())
    }

    /// Get the persisted Wayland portal restore token, if any.
    pub fn restore_token(&self) -> Option<&str> {
        self.config
            .portal
            .restore_token
            .as_deref()
            .filter(|t| !t.is_empty())
    }

    /// Persist a new Wayland portal restore token immediately.
    pub fn set_restore_token(&mut self, token: Option<String>) {
        self.config.portal.restore_token = token.filter(|t| !t.is_empty());
        if let Err(e) = self.save() {
            warn!("[Config] Failed to persist portal restore token: {}", e);
        }
    }

    pub fn last_region(&self) -> Option<Rectangle> {
        if self.config.capture.remember_region {
            self.config.capture.last_region
        } else {
            None
        }
    }

    pub fn set_last_region(&mut self, region: Rectangle) {
        self.config.capture.last_region = Some(region);
    }

    pub fn add_preset(
        &mut self,
        name: String,
        region: Rectangle,
        aspect_ratio: AspectRatio,
        default_action: PresetAction,
    ) -> RegionPreset {
        let preset = RegionPreset {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            region,
            aspect_ratio,
            default_action,
            fps: self.config.capture.frame_rate,
            show_cursor: self.config.capture.show_cursor,
        };
        self.config.upsert_preset(preset.clone());
        preset
    }

    pub fn reset_to_defaults(&mut self) {
        self.config = AppConfig::default();
    }
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self::load_default()
    }
}

/// Alias kept for gradual migration clarity.
pub type Config = ConfigStore;

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "region-to-share-cfg-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_v2_to_v3_migration() {
        let dir = unique_tmp_dir("v2");
        let v2_json = r#"{
            "frame_rate": 45,
            "capture_mode": "portal",
            "show_performance": true,
            "auto_send_to_background": true,
            "remember_last_region": true,
            "auto_use_specific_region": false,
            "last_region": { "x": 120, "y": 80, "width": 1600, "height": 900 },
            "language": "fr"
        }"#;
        fs::write(dir.join("settings.json"), v2_json).unwrap();

        let store = ConfigStore::load_from_dir(dir.clone());
        assert_eq!(store.config.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(store.config.capture.frame_rate, 45);
        assert_eq!(store.config.capture.backend, CaptureBackendKind::Portal);
        assert!(store.config.performance.show_performance_overlay);
        assert!(store.config.display.auto_background);
        assert_eq!(
            store.last_region(),
            Some(Rectangle::new(120, 80, 1600, 900))
        );
        assert_eq!(store.config.ui.language, "fr");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_atomic_save_restore_token_and_presets() {
        let dir = unique_tmp_dir("v3");
        let mut store = ConfigStore::load_from_dir(dir.clone());
        store.set_restore_token(Some("tok-abc-123".into()));
        store.add_preset(
            "Coding".into(),
            Rectangle::new(0, 0, 1440, 900),
            AspectRatio::Ratio16x10,
            PresetAction::Share,
        );
        store.save().unwrap();

        let reloaded = ConfigStore::load_from_dir(dir.clone());
        assert_eq!(reloaded.restore_token(), Some("tok-abc-123"));
        assert_eq!(reloaded.config.presets.len(), 1);
        assert_eq!(reloaded.config.presets[0].name, "Coding");
        let _ = fs::remove_dir_all(&dir);
    }
}
