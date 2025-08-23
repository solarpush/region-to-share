"""
Screen region selection module
Compatible with X11 and Wayland
"""

import os
import tempfile
from PyQt5.QtWidgets import (
    QWidget,
    QApplication,
    QLabel,
    QVBoxLayout,
    QRubberBand,
)
from PyQt5.QtCore import Qt, QRect, QPoint, pyqtSignal
from PyQt5.QtGui import QPainter, QColor, QPixmap, QIcon, QImage
from .config import config
import mss
from region_to_share.debug import debug_print


class ScreenSelector(QWidget):
    """Screen region selection widget"""

    selection_made = pyqtSignal(
        int, int, int, int, bool
    )  # x, y, width, height, reuse_last_region

    def __init__(self):
        super().__init__()

        # Set window icon
        self.set_window_icon()

        self.start_point = QPoint()
        self.end_point = QPoint()
        self.rubber_band = None
        self.screenshot = None
        self.last_region_overlay = None
        self.setup_ui()
        self.take_screenshot()
        self.show_last_region_if_enabled()

    def setup_ui(self):
        """User interface configuration"""
        self.setWindowTitle("Region Selection - Region to Share")

        # Make window fullscreen and borderless
        self.setWindowFlags(Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint)
        self.setWindowState(Qt.WindowFullScreen)

        # Cross cursor for selection
        self.setCursor(Qt.CrossCursor)

        # Main layout
        layout = QVBoxLayout()

        # Instructions
        self.instruction_label = QLabel(
            "Click and drag to select a screen region\n" "Press Esc to cancel"
        )
        self.instruction_label.setAlignment(Qt.AlignCenter)
        self.instruction_label.setStyleSheet(
            """
            QLabel {
                background-color: rgba(0, 0, 0, 180);
                color: white;
                font-size: 16px;
                padding: 10px;
                border-radius: 5px;
            }
        """
        )

        # Position instructions at the top
        layout.addWidget(self.instruction_label)
        layout.addStretch()

        self.setLayout(layout)

        # Rubber band for selection
        self.rubber_band = QRubberBand(QRubberBand.Rectangle, self)

    def set_window_icon(self):
        """Set the window icon from the application icon"""
        # Try different icon paths (snap, local dev, system)
        icon_paths = [
            "/snap/region-to-share/current/usr/share/icons/hicolor/64x64/apps/region-to-share.png",
            "region-to-share-64.png",  # Local development
            "region-to-share.png",
            "/usr/share/icons/hicolor/64x64/apps/region-to-share.png",
            "/usr/share/pixmaps/region-to-share.png",
        ]

        for icon_path in icon_paths:
            if os.path.exists(icon_path):
                icon = QIcon(icon_path)
                if not icon.isNull():
                    self.setWindowIcon(icon)
                    return

        # If no icon found, use a fallback from Qt resources if available
        debug_print("⚠️  Could not find application icon for selector")

    def take_screenshot(self):
        """Takes a screenshot of all monitors - X11 compatible"""
        try:
            # Method 1: Try Qt method (works on X11, limited on Wayland)
            if self._try_qt_screenshot():
                return

            # Method 2: Try MSS (usually works on X11)
            if self._try_mss_screenshot():
                return

            # Fallback: no screenshot
            self._fallback_no_screenshot()

        except Exception as e:
            debug_print(f"Error taking screenshot: {e}")
            self._fallback_no_screenshot()

    def _try_mss_screenshot(self):
        """Try MSS screenshot method - direct conversion without PIL"""
        try:
            with mss.mss() as sct:
                # Get all monitors bounding box
                monitor = sct.monitors[0]  # All monitors combined
                screenshot = sct.grab(monitor)

                # Convert MSS screenshot directly to QPixmap using raw data
                # Create QImage from raw data
                img = QImage(
                    screenshot.rgb,
                    screenshot.width,
                    screenshot.height,
                    QImage.Format_RGB888,
                )

                # Convert to QPixmap
                self.screenshot = QPixmap.fromImage(img)

                if not self.screenshot.isNull():
                    debug_print("✅ Screenshot successful (MSS)")
                    return True
                else:
                    return False
        except Exception as e:
            debug_print(f"MSS screenshot failed: {e}")
            return False

    def _try_qt_screenshot(self):
        """Try Qt screenshot method"""
        try:
            screen = QApplication.primaryScreen()
            self.screenshot = screen.grabWindow(QApplication.desktop().winId())
            if not self.screenshot.isNull():
                debug_print("✅ Screenshot successful (Qt)")
                return True
            else:
                debug_print("Qt screenshot returned null pixmap")
                return False
        except Exception as e:
            debug_print(f"Qt screenshot failed: {e}")
            return False

    def _fallback_no_screenshot(self):
        """Fallback: create a semi-transparent overlay without screenshot"""
        debug_print("⚠️  Screenshot not available, using transparent overlay")
        # Create a small transparent pixmap
        screen_geometry = QApplication.desktop().screenGeometry()
        self.screenshot = QPixmap(screen_geometry.size())
        self.screenshot.fill(QColor(20, 20, 20, 50))  # Dark semi-transparent

        # Update instructions
        self.instruction_label.setText(
            "Screenshot not available on this system\n"
            "Click and drag to select a screen region\n"
            "Press Esc to cancel"
        )

    def paintEvent(self, event):
        """Draws the screenshot as background"""
        painter = QPainter(self)

        if self.screenshot:
            # Draw the screenshot
            painter.drawPixmap(self.rect(), self.screenshot)

            # Darken the non-selected area
            if self.rubber_band and self.rubber_band.isVisible():
                # Selected area
                selected_rect = self.rubber_band.geometry()

                # Draw dark overlay everywhere except selection
                overlay = QColor(0, 0, 0, 100)
                painter.fillRect(self.rect(), overlay)

                # Clear overlay on selected area
                painter.setCompositionMode(QPainter.CompositionMode_Clear)
                painter.fillRect(selected_rect, Qt.transparent)

    def mousePressEvent(self, event):
        """Start of selection"""
        if event.button() == Qt.LeftButton and self.rubber_band:
            self.start_point = event.pos()
            self.rubber_band.setGeometry(QRect(self.start_point, self.start_point))
            self.rubber_band.show()

    def mouseMoveEvent(self, event):
        """Update selection"""
        if self.rubber_band and self.rubber_band.isVisible():
            self.rubber_band.setGeometry(
                QRect(self.start_point, event.pos()).normalized()
            )
            self.update()  # Redraw for overlay

    def mouseReleaseEvent(self, event):
        """End of selection"""
        if (
            event.button() == Qt.LeftButton
            and self.rubber_band
            and self.rubber_band.isVisible()
        ):
            self.end_point = event.pos()
            selection_rect = QRect(self.start_point, self.end_point).normalized()

            # Check that selection has minimum size
            if selection_rect.width() > 10 and selection_rect.height() > 10:
                self.selection_made.emit(
                    selection_rect.x(),
                    selection_rect.y(),
                    selection_rect.width(),
                    selection_rect.height(),
                    False,  # reuse_last_region = False for new selections
                )

            self.rubber_band.hide()

    def show_last_region_if_enabled(self):
        """Show last selected region if 'remember last region' is enabled"""
        if not config.get("remember_last_region"):
            return

        last_region = config.get("last_region")
        if not last_region:
            return

        try:
            x = int(last_region.get("x", 0))
            y = int(last_region.get("y", 0))
            width = int(last_region.get("width", 0))
            height = int(last_region.get("height", 0))

            if width > 0 and height > 0:
                # Create overlay to show last region
                self.last_region_overlay = QWidget(self)
                self.last_region_overlay.setGeometry(x, y, width, height)
                self.last_region_overlay.setStyleSheet(
                    """
                    QWidget {
                        border: 3px dashed #00FF00;
                        background-color: rgba(0, 255, 0, 30);
                    }
                    """
                )
                self.last_region_overlay.show()

                # Add instruction to reuse last region
                self.instruction_label.setText(
                    "Click and drag to select a new region\n"
                    "Press Enter to reuse last region (green outline)\n"
                    "Press Esc to cancel"
                )

        except Exception as e:
            debug_print(f"Error showing last region: {e}")

    def keyPressEvent(self, event):
        """Key handling"""
        if event.key() == Qt.Key_Escape:
            self.close()
        elif event.key() == Qt.Key_Return or event.key() == Qt.Key_Enter:
            # Reuse last region if available
            if config.get("remember_last_region"):
                last_region = config.get("last_region")
                if last_region:
                    x = int(last_region.get("x", 0))
                    y = int(last_region.get("y", 0))
                    width = int(last_region.get("width", 0))
                    height = int(last_region.get("height", 0))
                    if width > 0 and height > 0:
                        debug_print("🔄 Reusing last selected region")
                        self.selection_made.emit(
                            x, y, width, height, True
                        )  # reuse_last_region = True
                        self.close()
                        return
        super().keyPressEvent(event)
