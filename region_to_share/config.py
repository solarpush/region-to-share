"""
Configuration management for Region to Share
Handles user settings and preferences
"""

import os
import json
from pathlib import Path
from region_to_share.debug import debug_print


class Config:
    """Configuration manager for Region to Share"""

    def __init__(self):
        self.config_dir = Path.home() / ".config" / "region-to-share"
        self.config_file = self.config_dir / "settings.json"
        self.default_settings = {
            "frame_rate": 60,  # Changed from 30 to 60 for better performance
            "capture_mode": "auto",
            "show_performance": False,
            "window_opacity": 1.0,
            "auto_send_to_background": False,
            "remember_last_region": False,
            "auto_use_specific_region": False,
            "last_region": {"x": 0, "y": 0, "width": 800, "height": 600},
        }
        self.settings = self.load_settings()

    def load_settings(self):
        """Load settings from config file"""
        try:
            if self.config_file.exists():
                with open(self.config_file, "r") as f:
                    loaded = json.load(f)
                    # Merge with defaults to handle new settings
                    settings = self.default_settings.copy()
                    settings.update(loaded)
                    return settings
        except Exception as e:
            debug_print(f"⚠️ Error loading config: {e}")

        return self.default_settings.copy()

    def save_settings(self):
        """Save settings to config file"""
        try:
            self.config_dir.mkdir(parents=True, exist_ok=True)
            with open(self.config_file, "w") as f:
                json.dump(self.settings, f, indent=2)
            debug_print(f"💾 Settings saved to {self.config_file}")
        except Exception as e:
            debug_print(f"❌ Error saving config: {e}")

    def get(self, key, default=None):
        """Get a setting value"""
        return self.settings.get(key, default)

    def set(self, key, value):
        """Set a setting value"""
        self.settings[key] = value

    def reset_to_defaults(self):
        """Reset all settings to defaults"""
        self.settings = self.default_settings.copy()


# Global config instance
config = Config()
