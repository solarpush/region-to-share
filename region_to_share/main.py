#!/usr/bin/env python3
"""
Region to Share - Main Application
Allows selecting a screen area and sharing it via a transparent window
Compatible with X11 and Wayland
"""

import sys
import os
import signal
import argparse
from PyQt5.QtWidgets import QApplication
from PyQt5.QtCore import QTimer

# Ajouter le répertoire parent au path pour les imports
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from region_to_share.screen_selector import ScreenSelector
from region_to_share.display_window import DisplayWindow


class RegionToShareApp:
    def __init__(self, capture_mode=None):
        self.app = QApplication(sys.argv)
        self.screen_selector = None
        self.display_window = None
        self.capture_mode = capture_mode

        # Gérer la fermeture propre
        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)

        # Timer pour traiter les signaux
        self.timer = QTimer()
        self.timer.timeout.connect(lambda: None)
        self.timer.start(100)

    def signal_handler(self, signum, frame):
        """Signal handler for clean shutdown"""
        print("Shutdown signal received, cleaning up...")
        self.cleanup()
        self.app.quit()
        sys.exit(0)

    def cleanup(self):
        """Resource cleanup"""
        print("Cleaning up resources...")
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
        """Starts the region selection process"""
        self.screen_selector = ScreenSelector()
        self.screen_selector.selection_made.connect(self.on_selection_made)
        self.screen_selector.show()

    def on_selection_made(self, x, y, width, height):
        """Callback called when a region is selected"""
        print(f"Region selected: x={x}, y={y}, width={width}, height={height}")

        # Create display window with forced capture mode if specified
        self.display_window = DisplayWindow(
            x, y, width, height, capture_mode=self.capture_mode
        )
        self.display_window.closed.connect(self.cleanup)
        self.display_window.show()

        # Fermer le sélecteur
        if self.screen_selector:
            self.screen_selector.close()

    def run(self):
        """Launches the application"""
        self.start_selection()
        return self.app.exec_()


def main():
    """Main entry point"""
    parser = argparse.ArgumentParser(
        description="Region-to-Share: Select and share screen regions for video conferencing",
        epilog="Examples:\n"
        "  region-to-share                    # Auto-detect best method\n"
        "  region-to-share --mode mss         # Force MSS capture\n"
        "  region-to-share --mode grim        # Force grim capture\n"
        "  region-to-share --mode mutter-screencast  # Force Mutter ScreenCast\n",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    parser.add_argument(
        "--mode",
        "--capture-mode",
        choices=["auto", "mutter-screencast", "grim", "mss"],
        default="auto",
        help="Force a specific capture method (default: auto-detect)",
    )

    parser.add_argument("--debug", action="store_true", help="Enable debug output")

    parser.add_argument(
        "--version",
        action="version",
        version="Region-to-Share 1.0.5 - High-performance screen region capture for GNOME Wayland",
    )

    # Parse arguments (excluding Qt arguments)
    qt_args = []
    our_args = []
    for arg in sys.argv[1:]:
        if (
            arg.startswith("-style")
            or arg.startswith("-platform")
            or arg.startswith("-geometry")
        ):
            # Qt arguments - pass through
            qt_args.append(arg)
        else:
            our_args.append(arg)

    args = parser.parse_args(our_args)

    # Set debug mode
    if args.debug:
        os.environ["REGION_TO_SHARE_DEBUG"] = "1"
        print(f"🐛 Debug mode enabled")
        print(f"🔧 Forced capture mode: {args.mode}")

    # Create app with specified capture mode
    capture_mode = None if args.mode == "auto" else args.mode
    app = RegionToShareApp(capture_mode=capture_mode)
    return app.run()


if __name__ == "__main__":
    sys.exit(main())
