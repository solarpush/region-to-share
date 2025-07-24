"""
Screen region selection module
Compatible with X11 and Wayland
"""

import sys
from PyQt5.QtWidgets import (
    QWidget,
    QApplication,
    QLabel,
    QVBoxLayout,
    QHBoxLayout,
    QPushButton,
    QRubberBand,
    QDesktopWidget,
)
from PyQt5.QtCore import Qt, QRect, QPoint, pyqtSignal
from PyQt5.QtGui import QPainter, QPen, QColor, QPixmap, QScreen
import mss


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
        """Takes a screenshot of all monitors"""
        try:
            # Use Qt for screenshot (simpler and more reliable)
            screen = QApplication.primaryScreen()
            self.screenshot = screen.grabWindow(QApplication.desktop().winId())
            print("✅ Screenshot successful")

        except Exception as e:
            print(f"Error taking screenshot: {e}")
            # Fallback: black screenshot
            self.screenshot = QPixmap(1920, 1080)
            self.screenshot.fill(QColor(50, 50, 50))

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
