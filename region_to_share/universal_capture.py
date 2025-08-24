"""
Universal screen capture module for X11 and Wayland
Clean implementation focusing on what actually works
"""

import os

from typing import Optional
from PyQt5.QtGui import QPixmap, QImage, QPainter, QColor, QCursor, QPolygon
from PyQt5.QtCore import QPoint

from region_to_share.frame_profiler import (
    prof_total,
    prof_grab,
    prof_qimg,
    prof_qpx,
    prof_paint,
    now,
    NO_CONV,
)
from region_to_share.debug import debug_print

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

    def __init__(self, capture_mode=None, shared_portal=None):
        """
        Initialize UniversalCapture

        Args:
            capture_mode (str, optional): Force a specific capture method.
                Options: 'portal-screencast', 'mss', 'auto'
            shared_portal (object, optional): Existing portal instance to reuse
        """
        self.forced_mode = capture_mode
        self.shared_portal = shared_portal
        self._using_shared_portal = (
            shared_portal is not None
        )  # Flag pour traquer le portal partagé
        if capture_mode and capture_mode != "auto":
            self.capture_method = capture_mode
            debug_print(f"🔧 Forcing capture method: {self.capture_method}")
        else:
            self.capture_method = self._detect_best_method()
            debug_print(f"🎯 Using capture method: {self.capture_method}")

        self._temp_files = []
        self._portal_screencast = shared_portal  # Use shared portal if provided
        self._draw_cursor = (
            self.capture_method != "portal-screencast"
        )  # Portal doesn't support cursor drawing

        # For real FPS measurement
        self._last_capture_time = None

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
            debug_print(f"Erreur lors du dessin du curseur: {e}")

        return pixmap

    def _detect_best_method(self) -> str:
        """Detect the best capture method available"""
        session_type = os.environ.get("XDG_SESSION_TYPE", "").lower()
        in_snap = bool(os.environ.get("SNAP"))

        if session_type == "wayland":
            # En snap strict -> portail obligatoire
            if in_snap:
                if HAS_PORTAL:
                    return "portal-screencast"
                else:
                    debug_print("❌ Snap + Wayland nécessite le portail ScreenCast")
                    return "none"

            # Hors snap : essayer portail, sinon fallback MSS avec XWayland
            if HAS_PORTAL:
                debug_print("🔄 Wayland détecté, utilisation du portail ScreenCast")
                return "portal-screencast"
            else:
                debug_print(
                    "❌ Wayland détecté mais aucune méthode compatible disponible"
                )
                return "none"
        else:
            # Sur X11, préfère MSS pour les performances
            if HAS_MSS and self._test_mss():
                return "mss"
            elif HAS_PORTAL:
                debug_print("🔄 MSS indisponible, fallback portail")
                return "portal-screencast"
            else:
                debug_print("❌ X11 détecté mais aucune méthode disponible")
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
                    debug_print(
                        "❌ Portail ScreenCast forcé mais non disponible (dépendances manquantes)"
                    )
                    return None
                elif self.capture_method == "mss" and not HAS_MSS:
                    debug_print("❌ MSS forcé mais non disponible")
                    return None

            pixmap = None
            if self.capture_method == "portal-screencast":
                pixmap = self._capture_portal(x, y, width, height)
            elif self.capture_method == "mss":
                pixmap = self._capture_mss(x, y, width, height)
            else:
                debug_print("❌ Aucune méthode de capture disponible")
                return None

            # Dessiner le curseur sur le pixmap si activé
            if pixmap and self._draw_cursor:
                pixmap = self._draw_cursor_on_pixmap(pixmap, x, y)

            return pixmap
        except Exception as e:
            debug_print(f"❌ Échec capture: {e}")
            return None

    def _capture_portal(
        self, x: int, y: int, width: int, height: int
    ) -> Optional[QPixmap]:
        """Capture via portail XDG (compatible snap strict sous Wayland)"""
        try:
            t0 = now()

            # Mesurer le temps réel entre les captures
            if self._last_capture_time is not None:
                real_interval = t0 - self._last_capture_time
                # Utiliser prof_total pour le temps réel entre captures
                prof_total.push(real_interval)
            self._last_capture_time = t0

            if not self._portal_screencast:
                self._portal_screencast = PortalScreenCast()
                if not self._portal_screencast.start_area_capture(x, y, width, height):
                    debug_print("❌ Échec démarrage capture portail")
                    return None
            else:
                # Vérifier si la région a changé
                if self._portal_screencast._region != (x, y, width, height):
                    if self._using_shared_portal:
                        # Pour un portal partagé, juste changer la région de crop
                        debug_print(
                            f"🔄 Portal partagé - changement région crop vers {x},{y} {width}x{height}"
                        )
                        self._portal_screencast.set_crop_region(x, y, width, height)
                    else:
                        # Portal normal - on peut le nettoyer et redémarrer
                        self._portal_screencast.cleanup()
                        if not self._portal_screencast.start_area_capture(
                            x, y, width, height
                        ):
                            debug_print(
                                "❌ Échec redémarrage capture portail pour nouvelle région"
                            )
                            return None

            t1 = now()
            pixmap = self._portal_screencast.capture_frame()
            t2 = now()

            # Mesurer les temps de capture (pas le temps total)
            prof_grab.push(t1 - t0)  # Setup time
            prof_qimg.push(0)  # Portal gère l'image en interne
            prof_qpx.push(t2 - t1)  # Capture time
            prof_paint.push(0)  # Pas de paint direct

            return pixmap

        except Exception as e:
            debug_print(f"❌ Échec capture portail: {e}")
            return None

    def _capture_mss(
        self, x: int, y: int, width: int, height: int
    ) -> Optional[QPixmap]:
        """Capture using MSS (X11)"""
        try:
            t0 = now()

            # Mesurer le temps réel entre les captures
            if self._last_capture_time is not None:
                real_interval = t0 - self._last_capture_time
                # Utiliser prof_total pour le temps réel entre captures
                prof_total.push(real_interval)
            self._last_capture_time = t0

            if not hasattr(self, "_sct"):
                self._sct = mss.mss()

            # Grab screen region
            region = {"top": y, "left": x, "width": width, "height": height}
            screenshot = self._sct.grab(region)
            t1 = now()

            # Convert to QImage using buffer persistant (optimisé)
            bgra_size = width * height * 4
            if not hasattr(self, "_qimage_buf") or len(self._qimage_buf) != bgra_size:
                self._qimage_buf = bytearray(bgra_size)

            # Copy directly to avoid intermediate allocations
            self._qimage_buf[:] = screenshot.bgra

            # Format natif BGRA pour éviter conversion
            img = QImage(
                self._qimage_buf,
                width,
                height,
                width * 4,
                QImage.Format_ARGB32,  # Format natif le plus proche
            )
            t2 = now()

            # Créer un nouveau QPixmap à chaque fois (plus stable)
            pixmap = QPixmap.fromImage(img, NO_CONV)
            t3 = now()

            # Mesurer seulement les temps de capture (pas le temps total qui est mesuré plus haut)
            prof_grab.push(t1 - t0)
            prof_qimg.push(t2 - t1)
            prof_qpx.push(t3 - t2)
            prof_paint.push(0)  # Pas de paint dans cette version

            return pixmap  # Retourner directement le pixmap (pas de copie)

        except Exception as e:
            debug_print(f"MSS capture failed: {e}")
            return None

    def cleanup(self):
        """Clean up temporary files"""
        # Clean up MSS instance
        if hasattr(self, "_sct"):
            try:
                self._sct.close()
            except Exception:
                pass
            delattr(self, "_sct")

        # Clean up Portal ScreenCast if active (but only if it's not shared)
        if self._portal_screencast and not self._using_shared_portal:
            debug_print(
                f"🧹 Nettoyage portal UniversalCapture (ID: {id(self._portal_screencast)})"
            )
            self._portal_screencast.cleanup()
            self._portal_screencast = None
        elif self._using_shared_portal:
            debug_print(
                f"🔒 Portal partagé préservé dans UniversalCapture (ID: {id(self._portal_screencast)})"
            )
            # Ne pas nettoyer le portal partagé, juste détacher la référence
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
def create_capture(capture_mode=None, shared_portal=None):
    """Create a new capture instance

    Args:
        capture_mode (str, optional): Force a specific capture method.
            Options: 'portal-screencast', 'mss', 'auto'
        shared_portal (object, optional): Existing portal instance to reuse
    """
    return UniversalCapture(capture_mode=capture_mode, shared_portal=shared_portal)
