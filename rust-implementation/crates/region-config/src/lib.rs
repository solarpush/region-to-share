use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Default for LastRegion {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub frame_rate: u32,
    pub capture_mode: String,
    pub show_performance: bool,
    pub window_opacity: f32,
    pub auto_send_to_background: bool,
    pub remember_last_region: bool,
    pub auto_use_specific_region: bool,
    pub last_region: LastRegion,
    pub global_shortcut: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            frame_rate: 60,
            capture_mode: "auto".to_string(),
            show_performance: false,
            window_opacity: 1.0,
            auto_send_to_background: false,
            remember_last_region: false,
            auto_use_specific_region: false,
            last_region: LastRegion::default(),
            global_shortcut: String::new(),
        }
    }
}

pub struct Config {
    config_dir: PathBuf,
    config_file: PathBuf,
    pub settings: Settings,
}

impl Config {
    pub fn new() -> Self {
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("region-to-share");
        
        let config_file = config_dir.join("settings.json");
        
        let settings = Self::load_settings(&config_file);
        
        Self {
            config_dir,
            config_file,
            settings,
        }
    }
    
    fn load_settings(config_file: &PathBuf) -> Settings {
        if config_file.exists() {
            match fs::read_to_string(config_file) {
                Ok(content) => {
                    match serde_json::from_str::<Settings>(&content) {
                        Ok(settings) => {
                            eprintln!("✅ Settings loaded from {:?}", config_file);
                            return settings;
                        }
                        Err(e) => {
                            eprintln!("⚠️ Error parsing config: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("⚠️ Error reading config: {}", e);
                }
            }
        }
        
        Settings::default()
    }
    
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(&self.config_dir)?;
        let content = serde_json::to_string_pretty(&self.settings)?;
        fs::write(&self.config_file, content)?;
        eprintln!("💾 Settings saved to {:?}", self.config_file);
        Ok(())
    }
    
    pub fn get_frame_rate(&self) -> u32 {
        self.settings.frame_rate
    }
    
    pub fn set_frame_rate(&mut self, frame_rate: u32) {
        self.settings.frame_rate = frame_rate;
    }
    
    pub fn get_window_opacity(&self) -> f32 {
        self.settings.window_opacity
    }
    
    pub fn set_window_opacity(&mut self, opacity: f32) {
        self.settings.window_opacity = opacity.clamp(0.0, 1.0);
    }
    
    pub fn get_last_region(&self) -> Option<LastRegion> {
        if self.settings.remember_last_region {
            Some(self.settings.last_region.clone())
        } else {
            None
        }
    }
    
    pub fn set_last_region(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.settings.last_region = LastRegion { x, y, width, height };
    }
    
    pub fn reset_to_defaults(&mut self) {
        self.settings = Settings::default();
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}
