from PyQt5.QtWidgets import (
    QDialog,
    QVBoxLayout,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QSpinBox,
    QComboBox,
    QCheckBox,
    QGroupBox,
    QSlider,
    QFormLayout,
    QDialogButtonBox,
)
from PyQt5.QtCore import Qt
from .config import config
from region_to_share.debug import debug_print


class SettingsDialog(QDialog):
    """Settings configuration dialog"""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Region to Share - Settings")
        self.setFixedSize(500, 550)  # Increased height from 500 to 550
        self.setup_ui()
        self.load_current_settings()

    def setup_ui(self):
        """Setup the settings interface"""
        layout = QVBoxLayout()

        # Performance settings
        perf_group = QGroupBox("Performance Settings")
        perf_layout = QFormLayout()

        # Frame rate setting
        self.frame_rate_spin = QSpinBox()
        self.frame_rate_spin.setRange(1, 240)
        self.frame_rate_spin.setSuffix(" FPS")
        self.frame_rate_spin.setToolTip("Default frame rate for capture ")
        perf_layout.addRow("Default Frame Rate (Max: 240 FPS):", self.frame_rate_spin)

        # Capture mode setting
        self.capture_mode_combo = QComboBox()
        self.capture_mode_combo.addItems(["auto", "mss", "portal-screencast"])
        self.capture_mode_combo.setToolTip("Default capture method")
        perf_layout.addRow("Capture Mode:", self.capture_mode_combo)

        # Performance monitoring
        self.show_perf_check = QCheckBox("Show performance statistics")
        perf_layout.addRow("", self.show_perf_check)

        perf_group.setLayout(perf_layout)
        layout.addWidget(perf_group)

        # Window settings
        window_group = QGroupBox("Window Settings")
        window_layout = QFormLayout()

        # Default opacity
        self.opacity_slider = QSlider(Qt.Horizontal)
        self.opacity_slider.setRange(8, 100)
        self.opacity_slider.setValue(100)
        self.opacity_slider.valueChanged.connect(self.update_opacity_label)
        self.opacity_label = QLabel("100%")

        opacity_layout = QHBoxLayout()
        opacity_layout.addWidget(self.opacity_slider)
        opacity_layout.addWidget(self.opacity_label)
        window_layout.addRow("Default Opacity:", opacity_layout)

        # Auto send to background
        self.auto_background_check = QCheckBox(
            "Auto send to background after selection"
        )
        window_layout.addRow("", self.auto_background_check)

        # Remember last region
        self.remember_region_check = QCheckBox("Remember last selected region")
        self.remember_region_check.stateChanged.connect(self.on_remember_region_changed)
        window_layout.addRow("", self.remember_region_check)

        # Auto use specific region
        self.auto_use_region_check = QCheckBox(
            "Auto use last selected region (skip selector)"
        )
        window_layout.addRow("", self.auto_use_region_check)

        window_group.setLayout(window_layout)
        layout.addWidget(window_group)

        # Shortcut section
        shortcut_group = QGroupBox("Desktop Integration")
        shortcut_layout = QVBoxLayout()

        self.create_shortcut_btn = QPushButton("Create Desktop Shortcut")
        self.create_shortcut_btn.clicked.connect(self.create_desktop_shortcut)
        shortcut_layout.addWidget(self.create_shortcut_btn)

        self.create_launcher_btn = QPushButton("Add to Application Menu")
        self.create_launcher_btn.clicked.connect(self.create_application_launcher)
        shortcut_layout.addWidget(self.create_launcher_btn)

        shortcut_group.setLayout(shortcut_layout)
        layout.addWidget(shortcut_group)

        # Buttons
        button_box = QDialogButtonBox(
            QDialogButtonBox.Ok
            | QDialogButtonBox.Cancel
            | QDialogButtonBox.RestoreDefaults
        )
        button_box.accepted.connect(self.save_settings)
        button_box.rejected.connect(self.reject)
        button_box.button(QDialogButtonBox.RestoreDefaults).clicked.connect(
            self.restore_defaults
        )

        layout.addWidget(button_box)
        self.setLayout(layout)

    def load_current_settings(self):
        """Load current settings into the interface"""
        self.frame_rate_spin.setValue(int(config.get("frame_rate") or 30))

        capture_mode = str(config.get("capture_mode") or "auto")
        index = self.capture_mode_combo.findText(capture_mode)
        if index >= 0:
            self.capture_mode_combo.setCurrentIndex(index)

        self.show_perf_check.setChecked(bool(config.get("show_performance") or False))

        opacity_value = config.get("window_opacity") or 1.0
        opacity = int(float(opacity_value) * 100)
        self.opacity_slider.setValue(opacity)
        self.update_opacity_label()

        self.auto_background_check.setChecked(
            bool(config.get("auto_send_to_background") or False)
        )
        self.remember_region_check.setChecked(
            bool(config.get("remember_last_region") or False)
        )
        self.auto_use_region_check.setChecked(
            bool(config.get("auto_use_specific_region") or False)
        )

        # Apply the dependency logic after loading values
        self.on_remember_region_changed(
            Qt.Checked if self.remember_region_check.isChecked() else Qt.Unchecked
        )

    def update_opacity_label(self):
        """Update opacity percentage label"""
        value = self.opacity_slider.value()
        self.opacity_label.setText(f"{value}%")

    def on_remember_region_changed(self, state):
        """Handle remember region checkbox state change"""
        if state == Qt.Checked:
            # Enable auto use region when remember is enabled
            self.auto_use_region_check.setEnabled(True)
        else:
            # Disable and uncheck auto use region when remember is disabled
            self.auto_use_region_check.setEnabled(False)
            self.auto_use_region_check.setChecked(False)

    def save_settings(self):
        """Save settings and close dialog"""
        config.set("frame_rate", self.frame_rate_spin.value())
        config.set("capture_mode", self.capture_mode_combo.currentText())
        config.set("show_performance", self.show_perf_check.isChecked())
        config.set("window_opacity", self.opacity_slider.value() / 100.0)
        config.set("auto_send_to_background", self.auto_background_check.isChecked())
        config.set("remember_last_region", self.remember_region_check.isChecked())
        config.set("auto_use_specific_region", self.auto_use_region_check.isChecked())

        config.save_settings()
        self.accept()

    def restore_defaults(self):
        """Restore default settings"""
        config.reset_to_defaults()
        self.load_current_settings()

    def create_desktop_shortcut(self):
        """Create a desktop shortcut"""
        try:
            import os
            from pathlib import Path

            desktop_path = Path.home() / "Desktop"
            if not desktop_path.exists():
                desktop_path = Path.home() / "Bureau"  # French desktop

            shortcut_path = desktop_path / "region-to-share.desktop"

            # Get the current script path
            current_dir = Path(__file__).parent.parent
            script_path = current_dir / "run.sh"

            desktop_content = f"""[Desktop Entry]
Version=1.0
Type=Application
Name=Region to Share
Comment=Share specific screen regions in video calls
Exec={script_path}
Icon={current_dir / "region-to-share.png"}
Terminal=false
Categories=Utility;Graphics;
Keywords=screen;share;region;video;conference;
"""

            with open(shortcut_path, "w") as f:
                f.write(desktop_content)

            # Make executable
            shortcut_path.chmod(0o755)

            debug_print(f"✅ Desktop shortcut created: {shortcut_path}")

        except Exception as e:
            debug_print(f"❌ Error creating desktop shortcut: {e}")

    def create_application_launcher(self):
        """Create application menu launcher"""
        try:
            import os
            from pathlib import Path

            # Create .desktop file in applications directory
            apps_dir = Path.home() / ".local" / "share" / "applications"
            apps_dir.mkdir(parents=True, exist_ok=True)

            launcher_path = apps_dir / "region-to-share.desktop"

            # Get the current script path
            current_dir = Path(__file__).parent.parent
            script_path = current_dir / "run.sh"

            desktop_content = f"""[Desktop Entry]
Version=1.0
Type=Application
Name=Region to Share
Comment=Share specific screen regions in video calls
Exec={script_path}
Icon={current_dir / "region-to-share.png"}
Terminal=false
Categories=Utility;Graphics;AudioVideo;
Keywords=screen;share;region;video;conference;capture;
StartupNotify=true
"""

            with open(launcher_path, "w") as f:
                f.write(desktop_content)

            # Make executable
            launcher_path.chmod(0o755)

            # Update desktop database
            os.system(
                "update-desktop-database ~/.local/share/applications/ 2>/dev/null"
            )

            debug_print(f"✅ Application launcher created: {launcher_path}")

        except Exception as e:
            debug_print(f"❌ Error creating application launcher: {e}")
