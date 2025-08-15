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
from PyQt5.QtGui import QPixmap, QImage, QPainter, QColor, QCursor, QPolygon
from PyQt5.QtCore import QPoint
import numpy as np

try:
    import mss

    HAS_MSS = True
except ImportError:
    HAS_MSS = False

try:
    from region_to_share.portal_screencast import PortalScreenCast

    HAS_PORTAL = True
except ImportError:
    try:
        from .portal_screencast import PortalScreenCast

        HAS_PORTAL = True
    except ImportError:
        HAS_PORTAL = False


class UniversalCapture:
    """Universal screen capture that works on both X11 and Wayland"""

    def __init__(self, capture_mode=None):
        """
        Initialize UniversalCapture

        Args:
            capture_mode (str, optional): Force a specific capture method.
                Options: 'portal-screencast', 'mss', 'auto'
        """
        self.forced_mode = capture_mode
        if capture_mode and capture_mode != "auto":
            self.capture_method = capture_mode
            print(f"🔧 Forcing capture method: {self.capture_method}")
        else:
            self.capture_method = self._detect_best_method()
            print(f"🎯 Using capture method: {self.capture_method}")

        self._temp_files = []
        self._portal_screencast = None
        self._draw_cursor = (
            self.capture_method != "portal-screencast"
        )  # Portal doesn't support cursor drawing

    def _draw_cursor_on_pixmap(
        self, pixmap: QPixmap, region_x: int, region_y: int
    ) -> QPixmap:
        """Dessine le curseur sur le pixmap aux coordonnées relatives à la région"""
        if not self._draw_cursor:
            return pixmap

        try:
            # Obtenir la position actuelle du curseur
            cursor_pos = QCursor.pos()

            # Calculer la position relative dans la région capturée
            relative_x = cursor_pos.x() - region_x
            relative_y = cursor_pos.y() - region_y

            # Vérifier si le curseur est dans la région
            if 0 <= relative_x <= pixmap.width() and 0 <= relative_y <= pixmap.height():
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

        return pixmap

    def _detect_best_method(self) -> str:
        """Detect the best capture method available"""
        session_type = os.environ.get("XDG_SESSION_TYPE", "").lower()
        in_snap = bool(os.environ.get("SNAP"))

        if session_type == "wayland":
            # En snap strict -> portail obligatoire
            if in_snap:
                return "portal-screencast"

            # Hors snap : portail préférable
            if HAS_PORTAL:
                return "portal-screencast"
            else:
                print("❌ Wayland détecté mais aucune méthode compatible disponible")
                return "none"
        else:
            # Sur X11, préfère MSS pour les performances
            if HAS_MSS and self._test_mss():
                return "mss"
            else:
                print("❌ X11 détecté mais MSS ne fonctionne pas")
                return "none"

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
                if self.capture_method == "portal-screencast" and not HAS_PORTAL:
                    print(
                        "❌ Portail ScreenCast forcé mais non disponible (dépendances manquantes)"
                    )
                    return None
                elif self.capture_method == "mss" and not HAS_MSS:
                    print("❌ MSS forcé mais non disponible")
                    return None

            pixmap = None
            if self.capture_method == "portal-screencast":
                pixmap = self._capture_portal(x, y, width, height)
            elif self.capture_method == "mss":
                pixmap = self._capture_mss(x, y, width, height)
            else:
                print("❌ Aucune méthode de capture disponible")
                return None

            # Dessiner le curseur sur le pixmap si activé
            if pixmap and self._draw_cursor:
                pixmap = self._draw_cursor_on_pixmap(pixmap, x, y)

            return pixmap
        except Exception as e:
            print(f"❌ Échec capture: {e}")
            return None

    def _capture_portal(
        self, x: int, y: int, width: int, height: int
    ) -> Optional[QPixmap]:
        """Capture via portail XDG (compatible snap strict sous Wayland)"""
        try:
            if not self._portal_screencast:
                self._portal_screencast = PortalScreenCast()
                if not self._portal_screencast.start_area_capture(x, y, width, height):
                    print("❌ Échec démarrage capture portail")
                    return None
            else:
                # Vérifier si la région a changé
                if self._portal_screencast._region != (x, y, width, height):
                    self._portal_screencast.cleanup()
                    if not self._portal_screencast.start_area_capture(
                        x, y, width, height
                    ):
                        print(
                            "❌ Échec redémarrage capture portail pour nouvelle région"
                        )
                        return None

            return self._portal_screencast.capture_frame()

        except Exception as e:
            print(f"❌ Échec capture portail: {e}")
            return None

    def _capture_mss(
        self, x: int, y: int, width: int, height: int
    ) -> Optional[QPixmap]:
        """Capture using MSS (X11)"""
        try:
            with mss.mss() as sct:
                region = {"top": y, "left": x, "width": width, "height": height}
                screenshot = sct.grab(region)

                # Convert to numpy array then to QPixmap
                # Utiliser les données BGRA complètes pour garder le curseur
                img_array = np.frombuffer(screenshot.bgra, dtype=np.uint8).copy()
                img_array = img_array.reshape((screenshot.height, screenshot.width, 4))

                # Convert BGRA to RGB (ignorer le canal alpha mais garder le curseur intégré)
                img_rgb = img_array[:, :, [2, 1, 0]]  # BGR to RGB, ignore A

                h, w, ch = img_rgb.shape
                bytes_per_line = ch * w
                qt_image = QImage(
                    img_rgb.data.tobytes(), w, h, bytes_per_line, QImage.Format_RGB888
                )

                return QPixmap.fromImage(qt_image)
        except Exception as e:
            print(f"MSS capture failed: {e}")
            return None

    def cleanup(self):
        """Clean up temporary files"""
        # Clean up Portal ScreenCast if active
        if self._portal_screencast:
            self._portal_screencast.cleanup()
            self._portal_screencast = None

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
            Options: 'portal-screencast', 'mss', 'auto'
    """
    return UniversalCapture(capture_mode=capture_mode)
