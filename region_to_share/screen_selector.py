"""
Module de sélection de zone d'écran
Compatible X11 et Wayland
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
    """Widget de sélection de zone d'écran"""

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
        """Configuration de l'interface utilisateur"""
        self.setWindowTitle("Sélection de zone - Region to Share")

        # Rendre la fenêtre en plein écran et sans bordures
        self.setWindowFlags(Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint)
        self.setWindowState(Qt.WindowFullScreen)

        # Cursor de croix pour la sélection
        self.setCursor(Qt.CrossCursor)

        # Layout principal
        layout = QVBoxLayout()

        # Instructions
        self.instruction_label = QLabel(
            "Cliquez et glissez pour sélectionner une zone de l'écran\n"
            "Appuyez sur Échap pour annuler"
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

        # Positionner les instructions en haut
        layout.addWidget(self.instruction_label)
        layout.addStretch()

        self.setLayout(layout)

        # Rubber band pour la sélection
        self.rubber_band = QRubberBand(QRubberBand.Rectangle, self)

    def take_screenshot(self):
        """Prend une capture d'écran de tous les moniteurs"""
        try:
            # Utiliser Qt pour la capture d'écran (plus simple et fiable)
            screen = QApplication.primaryScreen()
            self.screenshot = screen.grabWindow(QApplication.desktop().winId())
            print("✅ Capture d'écran réussie")

        except Exception as e:
            print(f"Erreur lors de la capture d'écran: {e}")
            # Fallback : screenshot noir
            self.screenshot = QPixmap(1920, 1080)
            self.screenshot.fill(QColor(50, 50, 50))

    def paintEvent(self, event):
        """Dessine la capture d'écran en arrière-plan"""
        painter = QPainter(self)

        if self.screenshot:
            # Dessiner la capture d'écran
            painter.drawPixmap(self.rect(), self.screenshot)

            # Assombrir la zone non sélectionnée
            if self.rubber_band and self.rubber_band.isVisible():
                # Zone sélectionnée
                selected_rect = self.rubber_band.geometry()

                # Dessiner l'overlay sombre partout sauf sur la sélection
                overlay = QColor(0, 0, 0, 100)
                painter.fillRect(self.rect(), overlay)

                # Effacer l'overlay sur la zone sélectionnée
                painter.setCompositionMode(QPainter.CompositionMode_Clear)
                painter.fillRect(selected_rect, Qt.transparent)

    def mousePressEvent(self, event):
        """Début de sélection"""
        if event.button() == Qt.LeftButton and self.rubber_band:
            self.start_point = event.pos()
            self.rubber_band.setGeometry(QRect(self.start_point, self.start_point))
            self.rubber_band.show()

    def mouseMoveEvent(self, event):
        """Mise à jour de la sélection"""
        if self.rubber_band and self.rubber_band.isVisible():
            self.rubber_band.setGeometry(
                QRect(self.start_point, event.pos()).normalized()
            )
            self.update()  # Redessiner pour l'overlay

    def mouseReleaseEvent(self, event):
        """Fin de sélection"""
        if (
            event.button() == Qt.LeftButton
            and self.rubber_band
            and self.rubber_band.isVisible()
        ):
            self.end_point = event.pos()
            selection_rect = QRect(self.start_point, self.end_point).normalized()

            # Vérifier que la sélection a une taille minimale
            if selection_rect.width() > 10 and selection_rect.height() > 10:
                self.selection_made.emit(
                    selection_rect.x(),
                    selection_rect.y(),
                    selection_rect.width(),
                    selection_rect.height(),
                )

            self.rubber_band.hide()

    def keyPressEvent(self, event):
        """Gestion des touches"""
        if event.key() == Qt.Key_Escape:
            self.close()
        super().keyPressEvent(event)
