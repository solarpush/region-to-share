"""
Display window to show the selected region
This window can be shared directly in video conferencing applications
"""

import os
import subprocess
from PyQt5.QtWidgets import (
    QWidget,
    QVBoxLayout,
    QLabel,
    QPushButton,
    QHBoxLayout,
    QApplication,
    QDesktopWidget,
)
from PyQt5.QtCore import Qt, QTimer, pyqtSignal, QRect, QPoint
from PyQt5.QtGui import QPixmap, QImage, QPainter, QColor, QFont, QCursor, QPolygon

# Import our universal capture module
from .universal_capture import create_capture
import subprocess
from PyQt5.QtWidgets import (
    QWidget,
    QVBoxLayout,
    QLabel,
    QPushButton,
    QHBoxLayout,
)
from PyQt5.QtWidgets import (
    QWidget,
    QVBoxLayout,
    QLabel,
    QPushButton,
    QHBoxLayout,
    QApplication,
    QDesktopWidget,
)
from PyQt5.QtCore import Qt, QTimer, pyqtSignal, QRect, QPoint
from PyQt5.QtGui import QPixmap, QImage, QPainter, QColor, QFont, QCursor, QPolygon
import mss
import numpy as np
import cv2


class DisplayWindow(QWidget):
    """Display window for the selected region"""

    closed = pyqtSignal()

    def __init__(self, x, y, width, height, capture_mode=None):
        super().__init__()
        self.region_x = x
        self.region_y = y
        self.region_width = width
        self.region_height = height
        self.is_capturing = True
        self.capture_timer = QTimer()
        self.capture_mode = capture_mode
        self.session_type = os.environ.get("XDG_SESSION_TYPE", "").lower()
        # Create universal capture instance with forced mode if specified
        self.capturer = create_capture(capture_mode=capture_mode)

        self.setup_ui()
        self.start_capture()

    def setup_ui(self):
        """User interface configuration"""
        # Window configuration
        self.setWindowTitle("Region to Share - Selected Region")
        self.setWindowFlags(Qt.Window)

        # Window size = exactly the size of captured region
        self.resize(self.region_width, self.region_height)

        # Main layout without margins
        layout = QVBoxLayout()
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Display area for capture (takes all space)
        self.display_label = QLabel()
        self.display_label.setAlignment(Qt.AlignCenter)  # type: ignore
        self.display_label.setStyleSheet(
            """
            QLabel {
                background-color: #000;
                border: none;
                margin: 0px;
                padding: 0px;
            }
        """
        )
        self.display_label.setText("Initialisation de la capture...")

        layout.addWidget(self.display_label)
        self.setLayout(layout)

        # Create overlay controls
        self.create_overlay_controls()

        # Center the window
        self.center_window()

    def create_overlay_controls(self):
        """Creates overlay controls over the image"""
        # Control bar at top (semi-transparent)
        self.control_bar = QWidget(self)
        self.control_bar.setFixedHeight(35)
        self.control_bar.setStyleSheet(
            """
            QWidget {
                background-color: rgba(0, 0, 0, 180);
                border: none;
                border-radius: 5px;
            }
        """
        )

        # Layout for controls
        control_layout = QHBoxLayout(self.control_bar)
        control_layout.setContentsMargins(5, 2, 5, 2)

        # Information about the region
        self.region_info_label = QLabel(f"{self.region_width}×{self.region_height}")

        # Capture status
        self.status_label = QLabel("🔴 Live")
        self.status_label.setStyleSheet(
            "QLabel { color: #4CAF50; font-weight: bold; font-size: 11px; }"
        )

        # Buttons
        self.pause_btn = QPushButton("⏸️")
        self.pause_btn.setFixedSize(25, 25)
        self.pause_btn.setStyleSheet(
            """
            QPushButton {
                background-color: rgba(255, 255, 255, 200);
                border: 1px solid white;
                border-radius: 12px;
                font-size: 12px;
                color: black;
            }
            QPushButton:hover {
                background-color: rgba(255, 255, 255, 255);
            }
        """
        )
        self.pause_btn.clicked.connect(self.toggle_capture)

        self.minimize_btn = QPushButton("🗕")
        self.minimize_btn.setFixedSize(25, 25)
        self.minimize_btn.setStyleSheet(self.pause_btn.styleSheet())
        self.minimize_btn.setToolTip("Minimize window")
        self.minimize_btn.clicked.connect(self.showMinimized)

        self.refresh_btn = QPushButton("🔄")
        self.refresh_btn.setFixedSize(25, 25)
        self.refresh_btn.setStyleSheet(self.pause_btn.styleSheet())
        self.refresh_btn.clicked.connect(self.force_refresh)

        self.close_btn = QPushButton("❌")
        self.close_btn.setFixedSize(25, 25)
        self.close_btn.setStyleSheet(self.pause_btn.styleSheet())
        self.close_btn.clicked.connect(self.close)

        # Add to layout
        control_layout.addWidget(self.region_info_label)
        control_layout.addWidget(self.status_label)
        control_layout.addStretch()
        control_layout.addWidget(self.pause_btn)
        control_layout.addWidget(self.minimize_btn)
        control_layout.addWidget(self.close_btn)
        if self.session_type == "x11":
            # add refreshButton (not supported by wayland)
            control_layout.addWidget(self.refresh_btn)

        # Position bar at top of window
        self.control_bar.move(5, 5)
        self.control_bar.resize(self.width() - 10, 35)

    def resizeEvent(self, event):
        """Reposition controls when resizing"""
        super().resizeEvent(event)
        if hasattr(self, "control_bar"):
            self.control_bar.resize(self.width() - 10, 35)

    def center_window(self):
        """Centers the window on screen"""
        screen = self.screen().availableGeometry()
        window = self.frameGeometry()
        window.moveCenter(screen.center())
        self.move(window.topLeft())

    def start_capture(self):
        """Starts periodic capture"""
        self.capture_timer.timeout.connect(self.capture_frame)
        self.capture_timer.start(33)  # ~30 FPS (33ms)

    def _force_mss_x11_environment(self):
        """Force MSS to use X11 even on Wayland session"""
        if os.environ.get("XDG_SESSION_TYPE", "").lower() == "wayland":
            # Ensure DISPLAY is set for X11
            if not os.environ.get("DISPLAY"):
                # Try common X11 display values
                for display in [":0", ":1", ":10"]:
                    os.environ["DISPLAY"] = display
                    try:
                        # Quick test if this display works
                        import subprocess

                        subprocess.run(
                            ["xset", "q"],
                            check=True,
                            stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL,
                        )
                        print(f"✅ Using X11 display: {display}")
                        break
                    except:
                        continue
                else:
                    # Fallback to :0 if no test worked
                    os.environ["DISPLAY"] = ":0"
                    print("⚠️ Using fallback X11 display :0")

            # Temporarily disable Wayland for MSS
            self._original_wayland = os.environ.get("WAYLAND_DISPLAY")
            if "WAYLAND_DISPLAY" in os.environ:
                del os.environ["WAYLAND_DISPLAY"]

    def capture_frame(self):
        """Captures and displays a frame of the region using universal capture"""
        if not self.is_capturing:
            return

        try:
            # Use universal capture to get the region
            pixmap = self.capturer.capture_region(
                self.region_x, self.region_y, self.region_width, self.region_height
            )

            if pixmap and not pixmap.isNull():
                # Display the captured region
                self.display_label.setPixmap(
                    pixmap.scaled(
                        self.region_width,
                        self.region_height,
                        Qt.KeepAspectRatio,
                        Qt.SmoothTransformation,
                    )
                )
            else:
                self.display_label.setText("❌ Impossible de capturer l'écran")

        except Exception as e:
            self.display_label.setText(f"Erreur de capture: {e}")

    def closeEvent(self, event):
        """Cleanup on close"""
        self.capture_timer.stop()
        if hasattr(self, "capturer"):
            self.capturer.cleanup()
        self.closed.emit()
        super().closeEvent(event)

    def draw_cursor_on_pixmap(self, pixmap):
        """Draws the cursor on the pixmap"""
        try:
            # Get current cursor position
            cursor_pos = QCursor.pos()

            # Calculate relative position in captured region
            relative_x = cursor_pos.x() - self.region_x
            relative_y = cursor_pos.y() - self.region_y

            # Vérifier si le curseur est dans la région
            if (
                0 <= relative_x <= self.region_width
                and 0 <= relative_y <= self.region_height
            ):

                # Créer un painter pour dessiner sur le pixmap
                painter = QPainter(pixmap)
                painter.setRenderHint(QPainter.Antialiasing)

                # Dessiner une flèche de curseur simple
                cursor_size = 16

                # Couleur du curseur (blanc avec bordure noire)
                painter.setPen(QColor(0, 0, 0, 200))  # Bordure noire
                painter.setBrush(QColor(255, 255, 255, 220))  # Remplissage blanc

                # Dessiner la forme de flèche du curseur
                points = [
                    (relative_x, relative_y),
                    (relative_x, relative_y + cursor_size),
                    (relative_x + cursor_size // 3, relative_y + cursor_size * 2 // 3),
                    (
                        relative_x + cursor_size // 2,
                        relative_y + cursor_size * 2 // 3 + 2,
                    ),
                    (relative_x + cursor_size // 2, relative_y + cursor_size // 2),
                    (
                        relative_x + cursor_size * 2 // 3,
                        relative_y + cursor_size * 2 // 3,
                    ),
                    (relative_x + cursor_size, relative_y + cursor_size // 2),
                    (relative_x + cursor_size // 3, relative_y),
                ]

                # Convertir en QPolygon pour Qt
                qt_points = [QPoint(int(x), int(y)) for x, y in points]
                polygon = QPolygon(qt_points)

                # Dessiner le polygone de la flèche
                painter.drawPolygon(polygon)

                painter.end()

        except Exception as e:
            print(f"Erreur lors du dessin du curseur: {e}")

    def toggle_capture(self):
        """Enables/disables capture"""
        self.is_capturing = not self.is_capturing
        if self.is_capturing:
            self.pause_btn.setText("⏸️")
            self.status_label.setText("🔴 Live")
            self.status_label.setStyleSheet(
                "QLabel { color: #4CAF50; font-weight: bold; font-size: 11px; }"
            )
        else:
            self.pause_btn.setText("▶️")
            self.status_label.setText("⏸️ Pause")
            self.status_label.setStyleSheet(
                "QLabel { color: #FF9800; font-weight: bold; font-size: 11px; }"
            )

    def force_refresh(self):
        """Forces a capture refresh"""
        if not self.is_capturing:
            self.capture_frame()

    def update_region(self, x, y, width, height):
        """Updates the region to capture"""
        self.region_x = x
        self.region_y = y
        self.region_width = width
        self.region_height = height
        self.region_info_label.setText(f"Region: {width}×{height} px")

    def enterEvent(self, event):
        """Show controls on hover"""
        if hasattr(self, "control_bar"):
            self.control_bar.show()
        super().enterEvent(event)

    def leaveEvent(self, event):
        """Hide controls when mouse leaves"""
        if hasattr(self, "control_bar"):
            self.control_bar.hide()
        super().leaveEvent(event)

    # def closeEvent(self, a0):
    #     """Cleanup on close"""
    #     self.capture_timer.stop()
    #     self.closed.emit()
    #     super().closeEvent(a0)
