"""
Universal screen capture module for X11 and Wayland
Clean implementation focusing on what actually works
"""

import os
import sys
import tempfile
import subprocess
import shutil
import time
from typing import Optional
from PyQt5.QtGui import QPixmap, QImage
import numpy as np

try:
    import mss

    HAS_MSS = True
except ImportError:
    HAS_MSS = False

try:
    from region_to_share.mutter_screencast import (
        MutterScreenCastFast as MutterScreenCastOptimized,
        test_mutter_availability,
    )

    HAS_MUTTER = True
except ImportError:
    try:
        from .mutter_screencast import (
            MutterScreenCastFast as MutterScreenCastOptimized,
            test_mutter_availability,
        )

        HAS_MUTTER = True
    except ImportError:
        HAS_MUTTER = False


class UniversalCapture:
    """Universal screen capture that works on both X11 and Wayland"""

    def __init__(self, capture_mode=None):
        """
        Initialize UniversalCapture

        Args:
            capture_mode (str, optional): Force a specific capture method.
                Options: 'mutter-screencast', 'grim', 'mss', 'auto'
        """
        self.forced_mode = capture_mode
        if capture_mode and capture_mode != "auto":
            self.capture_method = capture_mode
            print(f"🔧 Forcing capture method: {self.capture_method}")
        else:
            self.capture_method = self._detect_best_method()
            print(f"🎯 Using capture method: {self.capture_method}")

        self._temp_files = []
        self._mutter_screencast = None

    def _detect_best_method(self) -> str:
        """Detect the best capture method available"""
        session_type = os.environ.get("XDG_SESSION_TYPE", "").lower()

        if session_type == "wayland":
            # On Wayland, detect compositor and use appropriate method
            desktop_session = os.environ.get("XDG_CURRENT_DESKTOP", "").lower()

            if "gnome" in desktop_session:
                # GNOME Wayland - try Mutter ScreenCast first (fastest), then grim fallback
                if HAS_MUTTER and test_mutter_availability():
                    return "mutter-screencast"
                elif self._has_grim():
                    return "grim"
                else:
                    print(
                        "❌ GNOME Wayland detected but no screenshot method available"
                    )
                    return "none"
            else:
                # Other Wayland compositors - try grim
                if self._has_grim():
                    return "grim"
                else:
                    print(
                        "❌ Wayland detected but no compatible screenshot tool available"
                    )
                    return "none"
        else:
            # On X11, prefer MSS for performance
            if HAS_MSS and self._test_mss():
                return "mss"
            else:
                print("❌ X11 detected but MSS not working")
                return "none"

    def _has_grim(self) -> bool:
        """Check if grim is available (Wayland screenshot tool)"""
        # First check system PATH
        if shutil.which("grim") is not None:
            return True

        # Then check if we have an embedded grim (for snap)
        snap_root = os.environ.get("SNAP", "")
        if snap_root:
            embedded_grim = os.path.join(snap_root, "bin", "grim")
            if os.path.exists(embedded_grim) and os.access(embedded_grim, os.X_OK):
                # Update PATH to include our embedded grim
                current_path = os.environ.get("PATH", "")
                grim_dir = os.path.dirname(embedded_grim)
                if grim_dir not in current_path.split(":"):
                    os.environ["PATH"] = f"{grim_dir}:{current_path}"
                return True

        return False

    def _test_mss(self) -> bool:
        """Test if MSS works"""
        if not HAS_MSS:
            return False
        try:
            with mss.mss() as sct:
                test_screenshot = sct.grab(
                    {"top": 0, "left": 0, "width": 1, "height": 1}
                )
                return test_screenshot is not None
        except Exception:
            return False

    def capture_region(
        self, x: int, y: int, width: int, height: int
    ) -> Optional[QPixmap]:
        """Capture a specific region of the screen"""
        try:
            # Validate forced mode availability
            if self.forced_mode and self.forced_mode != "auto":
                if self.capture_method == "mutter-screencast" and not HAS_MUTTER:
                    print(
                        "❌ Mutter ScreenCast forced but not available (missing D-Bus or GNOME)"
                    )
                    return None
                elif self.capture_method == "grim" and not self._has_grim():
                    print(
                        "❌ Grim forced but not available (install: sudo apt install grim)"
                    )
                    return None
                elif self.capture_method == "mss" and not HAS_MSS:
                    print("❌ MSS forced but not available")
                    return None

            if self.capture_method == "mutter-screencast":
                return self._capture_mutter_screencast(x, y, width, height)
            elif self.capture_method == "mss":
                return self._capture_mss(x, y, width, height)
            elif self.capture_method == "grim":
                return self._capture_grim(x, y, width, height)
            else:
                print("❌ No capture method available")
                return None
        except Exception as e:
            print(f"❌ Capture failed: {e}")
            return None

    def _capture_mutter_screencast(
        self, x: int, y: int, width: int, height: int
    ) -> Optional[QPixmap]:
        """Capture using GNOME Mutter ScreenCast (fastest for GNOME Wayland)"""
        try:
            # Initialize Mutter ScreenCast if not already done
            if not self._mutter_screencast:
                self._mutter_screencast = MutterScreenCastOptimized()

                # Initialize session
                if not self._mutter_screencast.initialize_session():
                    print("❌ Failed to initialize Mutter ScreenCast session")
                    return None

            # Start area capture if region changed
            current_region = getattr(self._mutter_screencast, "capture_region", None)
            if current_region != (x, y, width, height):
                if not self._mutter_screencast.start_area_capture(x, y, width, height):
                    print("❌ Failed to start area capture")
                    return None

                # Give it a moment to initialize
                time.sleep(0.1)

            # Capture frame
            return self._mutter_screencast.capture_frame()

        except Exception as e:
            print(f"Mutter ScreenCast capture failed: {e}")
            # Fallback to grim
            return self._capture_grim(x, y, width, height)

    def _capture_mss(
        self, x: int, y: int, width: int, height: int
    ) -> Optional[QPixmap]:
        """Capture using MSS (X11)"""
        try:
            with mss.mss() as sct:
                region = {"top": y, "left": x, "width": width, "height": height}
                screenshot = sct.grab(region)

                # Convert to numpy array then to QPixmap
                img_array = np.array(screenshot)
                # Convert BGRA to RGB
                img_rgb = img_array[:, :, [2, 1, 0]]  # BGR to RGB

                h, w, ch = img_rgb.shape
                bytes_per_line = ch * w
                qt_image = QImage(
                    img_rgb.data, w, h, bytes_per_line, QImage.Format_RGB888
                )

                return QPixmap.fromImage(qt_image)
        except Exception as e:
            print(f"MSS capture failed: {e}")
            return None

    def _capture_grim(
        self, x: int, y: int, width: int, height: int
    ) -> Optional[QPixmap]:
        """Capture using grim (Wayland)"""
        try:
            # Create temporary file
            temp_file = tempfile.mktemp(suffix=".png")
            self._temp_files.append(temp_file)

            # Use grim to capture specific region
            geometry = f"{x},{y} {width}x{height}"
            cmd = ["grim", "-g", geometry, temp_file]

            result = subprocess.run(cmd, capture_output=True, text=True, timeout=5)

            if result.returncode == 0 and os.path.exists(temp_file):
                pixmap = QPixmap(temp_file)
                return pixmap
            else:
                print(f"grim failed: {result.stderr}")
                return None

        except Exception as e:
            print(f"grim capture failed: {e}")
            return None

    def cleanup(self):
        """Clean up temporary files"""
        # Clean up Mutter ScreenCast if active
        if self._mutter_screencast:
            self._mutter_screencast.cleanup()
            self._mutter_screencast = None

        # Clean up temp files
        for temp_file in self._temp_files:
            try:
                if os.path.exists(temp_file):
                    os.remove(temp_file)
            except Exception:
                pass
        self._temp_files.clear()

    def __del__(self):
        """Cleanup when object is destroyed"""
        self.cleanup()


# Convenience function
def create_capture(capture_mode=None):
    """Create a new capture instance

    Args:
        capture_mode (str, optional): Force a specific capture method.
            Options: 'mutter-screencast', 'grim', 'mss', 'auto'
    """
    return UniversalCapture(capture_mode=capture_mode)
