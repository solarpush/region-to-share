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
    QLineEdit,
    QKeySequenceEdit,
)
from PyQt5.QtCore import Qt
from PyQt5.QtGui import QKeySequence
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
        self.frame_rate_spin.setToolTip(
            "Target frame rate for capture (depend the region w,h and your system capabilities)"
        )
        perf_layout.addRow("Target Frame Rate (Max: 240 FPS):", self.frame_rate_spin)

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
            "Auto send to background after selection (minimize on wayland)"
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

        # Keyboard shortcut selector
        shortcut_selector_layout = QHBoxLayout()
        shortcut_label = QLabel("Global Keyboard Shortcut: ")
        self.shortcut_edit = QKeySequenceEdit()
        self.shortcut_edit.setToolTip(
            "Set a global keyboard shortcut to launch the application (Ctrl+Meta+W or Other)"
        )
        self.clear_shortcut_btn = QPushButton("Clear")
        self.clear_shortcut_btn.clicked.connect(self.clear_shortcut)
        self.apply_shortcut_btn = QPushButton("Apply Shortcut")
        self.apply_shortcut_btn.clicked.connect(self.apply_global_shortcut)
        self.remove_shortcuts_btn = QPushButton("Remove All")
        self.remove_shortcuts_btn.clicked.connect(self.remove_all_shortcuts)
        self.remove_shortcuts_btn.setToolTip(
            "Remove all existing region-to-share shortcuts"
        )

        shortcut_selector_layout.addWidget(self.shortcut_edit)
        shortcut_selector_layout.addWidget(self.clear_shortcut_btn)
        shortcut_selector_layout.addWidget(self.apply_shortcut_btn)
        shortcut_selector_layout.addWidget(self.remove_shortcuts_btn)

        shortcut_layout.addWidget(shortcut_label)
        shortcut_layout.addLayout(shortcut_selector_layout)

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

        # Load keyboard shortcut
        shortcut_str = config.get("global_shortcut") or ""
        if shortcut_str:
            self.shortcut_edit.setKeySequence(QKeySequence(shortcut_str))

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
        config.set("global_shortcut", self.shortcut_edit.keySequence().toString())

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

            # Use snap command and icon
            snap_command = "region-to-share"
            current_dir = Path(__file__).parent.parent
            icon_path = current_dir / "region-to-share.png"

            desktop_content = f"""[Desktop Entry]
Version=1.0
Type=Application
Name=Region to Share
Comment=Share specific screen regions in video calls
Exec={snap_command}
Icon={icon_path}
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

            # Use snap command and icon
            snap_command = "region-to-share"
            current_dir = Path(__file__).parent.parent
            icon_path = current_dir / "region-to-share.png"

            desktop_content = f"""[Desktop Entry]
Version=1.0
Type=Application
Name=Region to Share
Comment=Share specific screen regions in video calls
Exec={snap_command}
Icon={icon_path}
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

    def clear_shortcut(self):
        """Clear the keyboard shortcut"""
        self.shortcut_edit.clear()

    def convert_shortcut_for_gsettings(self, shortcut):
        """Convert PyQt5 shortcut format to gsettings format"""
        # PyQt5 uses 'Meta' but gsettings expects 'Super'
        # Also convert from "Super+Ctrl+W" format to "<Super><Control>w" format
        converted = shortcut.replace("Meta+", "Super+")
        converted = converted.replace("Meta", "Super")

        # Convert to gsettings angle bracket format
        # Handle common modifiers
        gsettings_format = converted
        gsettings_format = gsettings_format.replace("Super+", "<Super>")
        gsettings_format = gsettings_format.replace("Ctrl+", "<Control>")
        gsettings_format = gsettings_format.replace("Alt+", "<Alt>")
        gsettings_format = gsettings_format.replace("Shift+", "<Shift>")

        # Convert the final key to lowercase
        parts = gsettings_format.split(">")
        if len(parts) > 1:
            # Last part is the key
            key = parts[-1].lower()
            gsettings_format = ">".join(parts[:-1]) + ">" + key

        debug_print(
            f"🔄 Format conversion: '{shortcut}' -> '{converted}' -> '{gsettings_format}'"
        )
        return gsettings_format

    def apply_global_shortcut(self):
        """Apply the global keyboard shortcut"""
        try:
            import subprocess
            from pathlib import Path

            shortcut = self.shortcut_edit.keySequence().toString()
            if not shortcut:
                debug_print("⚠️ No shortcut defined")
                return

            # Convert Meta to Super for gsettings compatibility
            shortcut_gsettings = self.convert_shortcut_for_gsettings(shortcut)
            debug_print(
                f"🔄 Converting shortcut: '{shortcut}' -> '{shortcut_gsettings}'"
            )

            # Use the snap command instead of local script
            snap_command = "region-to-share"

            # Create gsettings command for GNOME
            command_name = "region-to-share-launch"
            gsettings_path = "org.gnome.settings-daemon.plugins.media-keys"

            debug_print(f"🔧 Using snap command: '{snap_command}'")
            debug_print(f"🔧 Using gsettings path: '{gsettings_path}'")
            debug_print(f"🔧 Using command name: '{command_name}'")

            # First, remove any existing region-to-share shortcuts
            debug_print("🧹 Removing existing shortcuts...")
            self.remove_existing_shortcuts()

            # First, get current custom keybindings
            debug_print("📋 Getting current keybindings...")
            custom_keybindings = subprocess.run(
                ["gsettings", "get", gsettings_path, "custom-keybindings"],
                capture_output=True,
                text=True,
            )
            debug_print(f"📋 Current keybindings: {custom_keybindings.stdout.strip()}")

            custom_path = f"/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/{command_name}/"
            debug_print(f"🔧 Custom path: '{custom_path}'")

            # Set the custom keybinding properties
            debug_print("⚙️ Setting keybinding name...")
            name_result = subprocess.run(
                [
                    "gsettings",
                    "set",
                    f"{gsettings_path}.custom-keybinding:{custom_path}",
                    "name",
                    "Region to Share",
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            debug_print(
                f"✅ Name set - stdout: '{name_result.stdout}', stderr: '{name_result.stderr}'"
            )

            debug_print("⚙️ Setting keybinding command...")
            command_result = subprocess.run(
                [
                    "gsettings",
                    "set",
                    f"{gsettings_path}.custom-keybinding:{custom_path}",
                    "command",
                    snap_command,
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            debug_print(
                f"✅ Command set - stdout: '{command_result.stdout}', stderr: '{command_result.stderr}'"
            )

            debug_print("⚙️ Setting keybinding shortcut...")
            binding_result = subprocess.run(
                [
                    "gsettings",
                    "set",
                    f"{gsettings_path}.custom-keybinding:{custom_path}",
                    "binding",
                    shortcut_gsettings,
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            debug_print(
                f"✅ Binding set - stdout: '{binding_result.stdout}', stderr: '{binding_result.stderr}'"
            )

            # Add to the list of custom keybindings
            current_bindings = custom_keybindings.stdout.strip()
            debug_print(f"📝 Processing current bindings: '{current_bindings}'")

            if current_bindings == "@as []" or current_bindings == "[]":
                # No existing bindings
                new_bindings = f"['{custom_path}']"
                debug_print("📝 No existing bindings, creating new list")
            else:
                # Parse existing bindings more carefully
                # Remove brackets and split by comma, then clean each item
                current_bindings_clean = current_bindings.strip("[]@as ")
                debug_print(f"📝 Cleaned bindings: '{current_bindings_clean}'")

                if current_bindings_clean:
                    # Split by comma and clean each binding path
                    bindings_list = []
                    for binding in current_bindings_clean.split(","):
                        cleaned_binding = binding.strip().strip("'\"").strip()
                        if cleaned_binding and cleaned_binding not in bindings_list:
                            bindings_list.append(cleaned_binding)
                            debug_print(f"📝 Added binding: '{cleaned_binding}'")

                    # Add our binding if not already present
                    if custom_path not in bindings_list:
                        bindings_list.append(custom_path)
                        debug_print(f"📝 Added our binding: '{custom_path}'")
                    else:
                        debug_print(f"📝 Our binding already exists: '{custom_path}'")
                else:
                    bindings_list = [custom_path]
                    debug_print("📝 No valid existing bindings, using only ours")

                # Format the new bindings list
                new_bindings = (
                    "[" + ", ".join(f"'{binding}'" for binding in bindings_list) + "]"
                )

            debug_print(f"📝 Final bindings list: '{new_bindings}'")

            # Apply the updated bindings list
            debug_print("📝 Applying updated bindings list...")
            bindings_result = subprocess.run(
                [
                    "gsettings",
                    "set",
                    gsettings_path,
                    "custom-keybindings",
                    new_bindings,
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            debug_print(
                f"✅ Bindings applied - stdout: '{bindings_result.stdout}', stderr: '{bindings_result.stderr}'"
            )

            # Test the command manually
            debug_print(f"🧪 Testing command manually: '{snap_command}'")
            test_result = subprocess.run(
                [snap_command, "--help"], capture_output=True, text=True, timeout=5
            )
            debug_print(
                f"🧪 Test result - returncode: {test_result.returncode}, stdout: '{test_result.stdout[:100]}...', stderr: '{test_result.stderr[:100]}...'"
            )

            # Force reload of settings daemon
            debug_print("🔄 Reloading settings daemon...")
            reload_result = subprocess.run(
                ["killall", "-USR1", "gnome-settings-daemon"],
                capture_output=True,
                text=True,
            )
            debug_print(
                f"🔄 Reload result - returncode: {reload_result.returncode}, stderr: '{reload_result.stderr}'"
            )

            # Verify the shortcut was created
            debug_print("✅ Verifying shortcut creation...")
            verify_result = subprocess.run(
                [
                    "gsettings",
                    "get",
                    f"{gsettings_path}.custom-keybinding:{custom_path}",
                    "binding",
                ],
                capture_output=True,
                text=True,
            )
            debug_print(f"✅ Verification - binding: {verify_result.stdout.strip()}")

            debug_print(
                f"✅ Global shortcut '{shortcut}' -> '{shortcut_gsettings}' applied successfully"
            )

        except subprocess.CalledProcessError as e:
            debug_print(f"❌ Error running gsettings command: {e}")
            debug_print("💡 Note: Global shortcuts require GNOME desktop environment")
        except Exception as e:
            debug_print(f"❌ Error applying global shortcut: {e}")
            debug_print("💡 Note: Global shortcuts require GNOME desktop environment")

    def remove_existing_shortcuts(self):
        """Remove existing region-to-share shortcuts"""
        try:
            import subprocess

            gsettings_path = "org.gnome.settings-daemon.plugins.media-keys"

            # Get current keybindings
            result = subprocess.run(
                ["gsettings", "get", gsettings_path, "custom-keybindings"],
                capture_output=True,
                text=True,
            )

            current_bindings = result.stdout.strip()
            if current_bindings in ["@as []", "[]"]:
                return

            # Parse and filter out region-to-share bindings
            current_bindings_clean = current_bindings.strip("[]@as ")
            if current_bindings_clean:
                bindings_list = []
                for binding in current_bindings_clean.split(","):
                    cleaned_binding = binding.strip().strip("'\"").strip()
                    if cleaned_binding and "region-to-share" not in cleaned_binding:
                        bindings_list.append(cleaned_binding)

                # Update the bindings list
                if bindings_list:
                    new_bindings = (
                        "["
                        + ", ".join(f"'{binding}'" for binding in bindings_list)
                        + "]"
                    )
                else:
                    new_bindings = "[]"

                subprocess.run(
                    [
                        "gsettings",
                        "set",
                        gsettings_path,
                        "custom-keybindings",
                        new_bindings,
                    ],
                    check=True,
                )

                debug_print("🧹 Existing region-to-share shortcuts removed")

        except Exception as e:
            debug_print(f"⚠️ Warning: Could not clean existing shortcuts: {e}")

    def remove_all_shortcuts(self):
        """Remove all region-to-share shortcuts and clear the input field"""
        self.remove_existing_shortcuts()
        self.shortcut_edit.clear()
        debug_print("🗑️ All region-to-share shortcuts removed")
