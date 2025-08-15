"""
Screen region selection module
Compatible with X11 and Wayland
"""

import sys
import os
import tempfile
from PyQt5.QtWidgets import (
    QWidget,
    QApplication,
    QLabel,
    QVBoxLayout,
    QHBoxLayout,
    QPushButton,
    QRubberBand,
    QDesktopWidget,
    QMessageBox,
)
from PyQt5.QtCore import Qt, QRect, QPoint, pyqtSignal
from PyQt5.QtGui import QPainter, QPen, QColor, QPixmap, QScreen
import mss

# Import pyscreenshot as fallback for Wayland
try:
    import pyscreenshot as ImageGrab
    from PIL import Image

    HAS_PYSCREENSHOT = True
except ImportError:
    HAS_PYSCREENSHOT = False
    print("⚠️  pyscreenshot not available")


class ScreenSelector(QWidget):
    """Screen region selection widget"""

    selection_made = pyqtSignal(int, int, int, int)  # x, y, width, height

    def __init__(self):
        super().__init__()
        self.start_point = QPoint()
        self.end_point = QPoint()
        self.rubber_band = None
        self.screenshot = None
        self.setup_ui()
        self.take_screenshot()

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

    def take_screenshot(self):
        """Takes a screenshot of all monitors - Wayland and X11 compatible"""
        try:
            # Method 1: Try pyscreenshot (works on both Wayland and X11)
            if HAS_PYSCREENSHOT and self._try_pyscreenshot():
                return

            # Method 2: Try Qt method (works on X11, limited on Wayland)
            if self._try_qt_screenshot():
                return

            # Method 3: Try MSS (usually works on X11)
            if self._try_mss_screenshot():
                return

            # Fallback: no screenshot
            self._fallback_no_screenshot()

        except Exception as e:
            print(f"Error taking screenshot: {e}")
            self._fallback_no_screenshot()

    def _try_pyscreenshot(self):
        """Try pyscreenshot method (Wayland compatible)"""
        try:
            # Use pyscreenshot to capture the screen
            im = ImageGrab.grab()

            # Convert PIL Image to QPixmap
            temp_file = tempfile.mktemp(suffix=".png")
            im.save(temp_file, "PNG")  # type:ignore

            self.screenshot = QPixmap(temp_file)
            os.remove(temp_file)  # Clean up

            if not self.screenshot.isNull():
                print("✅ Screenshot successful (pyscreenshot)")
                return True
            else:
                return False

        except Exception as e:
            print(f"pyscreenshot failed: {e}")
            return False

    def _try_mss_screenshot(self):
        """Try MSS screenshot method"""
        try:
            with mss.mss() as sct:
                # Get all monitors bounding box
                monitor = sct.monitors[0]  # All monitors combined
                screenshot = sct.grab(monitor)

                # Convert to PIL Image then save and load as QPixmap
                from PIL import Image

                im = Image.frombytes(
                    "RGB", screenshot.size, screenshot.bgra, "raw", "BGRX"
                )

                temp_file = tempfile.mktemp(suffix=".png")
                im.save(temp_file, "PNG")

                self.screenshot = QPixmap(temp_file)
                os.remove(temp_file)  # Clean up

                if not self.screenshot.isNull():
                    print("✅ Screenshot successful (MSS)")
                    return True
                else:
                    return False
        except Exception as e:
            print(f"MSS screenshot failed: {e}")
            return False

    def _try_qt_screenshot(self):
        """Try Qt screenshot method"""
        try:
            screen = QApplication.primaryScreen()
            self.screenshot = screen.grabWindow(QApplication.desktop().winId())
            if not self.screenshot.isNull():
                print("✅ Screenshot successful (Qt)")
                return True
            else:
                print("Qt screenshot returned null pixmap")
                return False
        except Exception as e:
            print(f"Qt screenshot failed: {e}")
            return False

    def _fallback_no_screenshot(self):
        """Fallback: create a semi-transparent overlay without screenshot"""
        print("⚠️  Screenshot not available, using transparent overlay")
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
                )

            self.rubber_band.hide()

    def keyPressEvent(self, event):
        """Key handling"""
        if event.key() == Qt.Key_Escape:
            self.close()
        super().keyPressEvent(event)
