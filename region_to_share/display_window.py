"""
Fenêtre d'affichage pour montrer la zone sélectionnée
Cette fenêtre peut être partagée directement dans les applications de visioconférence
"""

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
    """Fenêtre d'affichage pour la zone sélectionnée"""

    closed = pyqtSignal()

    def __init__(self, x, y, width, height):
        super().__init__()
        self.region_x = x
        self.region_y = y
        self.region_width = width
        self.region_height = height
        self.is_capturing = True
        self.capture_timer = QTimer()
        self.setup_ui()
        self.start_capture()

    def setup_ui(self):
        """Configuration de l'interface utilisateur"""
        # Configuration de la fenêtre
        self.setWindowTitle("Region to Share - Zone Sélectionnée")
        self.setWindowFlags(Qt.Window)

        # Taille de la fenêtre = exactement la taille de la région capturée
        self.resize(self.region_width, self.region_height)

        # Layout principal sans marges
        layout = QVBoxLayout()
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Zone d'affichage de la capture (prend tout l'espace)
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

        # Créer les contrôles en overlay
        self.create_overlay_controls()

        # Centrer la fenêtre
        self.center_window()

    def create_overlay_controls(self):
        """Crée les contrôles en overlay par-dessus l'image"""
        # Barre de contrôle en haut (semi-transparente)
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

        # Layout pour les contrôles
        controls_layout = QHBoxLayout(self.control_bar)
        controls_layout.setContentsMargins(8, 4, 8, 4)

        # Informations sur la zone
        self.info_label = QLabel(f"Zone: {self.region_width}×{self.region_height} px")
        self.info_label.setStyleSheet("QLabel { color: white; font-size: 11px; }")

        # Statut de capture
        self.status_label = QLabel("🔴 Live")
        self.status_label.setStyleSheet(
            "QLabel { color: #4CAF50; font-weight: bold; font-size: 11px; }"
        )

        # Boutons
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

        self.refresh_btn = QPushButton("🔄")
        self.refresh_btn.setFixedSize(25, 25)
        self.refresh_btn.setStyleSheet(self.pause_btn.styleSheet())
        self.refresh_btn.clicked.connect(self.force_refresh)

        self.close_btn = QPushButton("❌")
        self.close_btn.setFixedSize(25, 25)
        self.close_btn.setStyleSheet(self.pause_btn.styleSheet())
        self.close_btn.clicked.connect(self.close)

        # Ajouter au layout
        controls_layout.addWidget(self.info_label)
        controls_layout.addWidget(self.status_label)
        controls_layout.addStretch()
        controls_layout.addWidget(self.pause_btn)
        controls_layout.addWidget(self.refresh_btn)
        controls_layout.addWidget(self.close_btn)

        # Positionner la barre en haut de la fenêtre
        self.control_bar.move(5, 5)
        self.control_bar.resize(self.width() - 10, 35)

        # Instruction en bas (cachée par défaut)
        self.instruction_label = QLabel(
            "💡 Partagez cette fenêtre dans Google Meet/Teams", self
        )
        self.instruction_label.setStyleSheet(
            """
            QLabel {
                background-color: rgba(0, 0, 0, 180);
                color: white;
                font-size: 10px;
                padding: 5px 10px;
                border-radius: 5px;
            }
        """
        )
        self.instruction_label.adjustSize()
        self.instruction_label.move(
            5, self.height() - self.instruction_label.height() - 5
        )
        self.instruction_label.hide()  # Caché par défaut

    def resizeEvent(self, event):
        """Repositionner les contrôles lors du redimensionnement"""
        super().resizeEvent(event)
        if hasattr(self, "control_bar"):
            self.control_bar.resize(self.width() - 10, 35)
        if hasattr(self, "instruction_label"):
            self.instruction_label.move(
                5, self.height() - self.instruction_label.height() - 5
            )

    def center_window(self):
        """Centre la fenêtre sur l'écran"""
        screen = self.screen().availableGeometry()
        window = self.frameGeometry()
        window.moveCenter(screen.center())
        self.move(window.topLeft())

    def start_capture(self):
        """Démarre la capture périodique"""
        self.capture_timer.timeout.connect(self.capture_frame)
        self.capture_timer.start(33)  # ~30 FPS (33ms)

    def capture_frame(self):
        """Capture et affiche une frame de la région avec curseur manuel"""
        if not self.is_capturing:
            return

        try:
            # Capturer la région avec mss
            with mss.mss() as sct:
                region = {
                    "top": self.region_y,
                    "left": self.region_x,
                    "width": self.region_width,
                    "height": self.region_height,
                }
                screenshot = sct.grab(region)

                # Convertir en format Qt
                img_array = np.array(screenshot)
                img_rgb = cv2.cvtColor(img_array, cv2.COLOR_BGRA2RGB)

                h, w, ch = img_rgb.shape
                bytes_per_line = ch * w

                qt_image = QImage(
                    img_rgb.data, w, h, bytes_per_line, QImage.Format_RGB888
                )

                # Créer un pixmap et dessiner le curseur dessus
                pixmap = QPixmap.fromImage(qt_image)

                # Ajouter le curseur manuellement
                self.draw_cursor_on_pixmap(pixmap)

                # Afficher le pixmap avec curseur
                self.display_label.setPixmap(pixmap)

        except Exception as e:
            self.display_label.setText(f"Erreur de capture: {e}")

    def draw_cursor_on_pixmap(self, pixmap):
        """Dessine le curseur sur le pixmap"""
        try:
            # Obtenir la position actuelle du curseur
            cursor_pos = QCursor.pos()

            # Calculer la position relative dans la région capturée
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
                    (relative_x + cursor_size // 2, relative_y + cursor_size // 2),
                    (relative_x + cursor_size * 2 // 3, relative_y + cursor_size // 3),
                    (relative_x + cursor_size, relative_y),
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
        """Active/désactive la capture"""
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
        """Force une actualisation de la capture"""
        if not self.is_capturing:
            self.capture_frame()

    def update_region(self, x, y, width, height):
        """Met à jour la région à capturer"""
        self.region_x = x
        self.region_y = y
        self.region_width = width
        self.region_height = height
        self.info_label.setText(f"Zone: {width}×{height} px")

    def enterEvent(self, event):
        """Afficher les contrôles au survol"""
        if hasattr(self, "control_bar"):
            self.control_bar.show()
        if hasattr(self, "instruction_label"):
            self.instruction_label.show()
        super().enterEvent(event)

    def leaveEvent(self, event):
        """Masquer les contrôles quand la souris sort"""
        if hasattr(self, "control_bar"):
            self.control_bar.hide()
        if hasattr(self, "instruction_label"):
            self.instruction_label.hide()
        super().leaveEvent(event)

    def closeEvent(self, a0):
        """Nettoyage à la fermeture"""
        self.capture_timer.stop()
        self.closed.emit()
        super().closeEvent(a0)
