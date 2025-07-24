#!/usr/bin/env python3
"""
Region to Share - Main Application
Permet de sélectionner une zone d'écran et de la partager via une fenêtre transparente
Compatible X11 et Wayland
"""

import sys
import os
import signal
from PyQt5.QtWidgets import QApplication
from PyQt5.QtCore import QTimer

# Ajouter le répertoire parent au path pour les imports
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from region_to_share.screen_selector import ScreenSelector
from region_to_share.display_window import DisplayWindow


class RegionToShareApp:
    def __init__(self):
        self.app = QApplication(sys.argv)
        self.screen_selector = None
        self.display_window = None

        # Gérer la fermeture propre
        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)

        # Timer pour traiter les signaux
        self.timer = QTimer()
        self.timer.timeout.connect(lambda: None)
        self.timer.start(100)

    def signal_handler(self, signum, frame):
        """Gestionnaire de signaux pour fermeture propre"""
        print("Signal de fermeture reçu, nettoyage en cours...")
        self.cleanup()
        self.app.quit()
        sys.exit(0)

    def cleanup(self):
        """Nettoyage des ressources"""
        print("Nettoyage des ressources...")
        if self.display_window:
            self.display_window.close()
            self.display_window = None
        if self.screen_selector:
            self.screen_selector.close()
            self.screen_selector = None
        # Quitter l'application
        if hasattr(self, "app"):
            self.app.quit()

    def start_selection(self):
        """Démarre le processus de sélection de zone"""
        self.screen_selector = ScreenSelector()
        self.screen_selector.selection_made.connect(self.on_selection_made)
        self.screen_selector.show()

    def on_selection_made(self, x, y, width, height):
        """Callback appelé quand une zone est sélectionnée"""
        print(f"Zone sélectionnée: x={x}, y={y}, width={width}, height={height}")

        # Créer la fenêtre d'affichage
        self.display_window = DisplayWindow(x, y, width, height)
        self.display_window.closed.connect(self.cleanup)
        self.display_window.show()

        # Fermer le sélecteur
        if self.screen_selector:
            self.screen_selector.close()

    def run(self):
        """Lance l'application"""
        self.start_selection()
        return self.app.exec_()


def main():
    """Point d'entrée principal"""
    app = RegionToShareApp()
    return app.run()


if __name__ == "__main__":
    sys.exit(main())
