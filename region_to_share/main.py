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

from region_to_share.frame_profiler import (
    dump_stats,
    get_stats_formatted,
    init_cpu_monitoring,
    enable_profiling,
    disable_profiling,
)
from region_to_share.screen_selector import ScreenSelector
from region_to_share.display_window import DisplayWindow
from region_to_share.config import config
from region_to_share.debug import debug_print


class RegionToShareApp:
    def __init__(self, capture_mode=None, show_perf=False, frame_rate=30):
        debug_print(
            f"Initializing RegionToShareApp: capture_mode={capture_mode}, show_perf={show_perf}, frame_rate={frame_rate}"
        )
        self.app = QApplication(sys.argv)
        self.screen_selector = None
        self.display_window = None
        self.capture_mode = capture_mode
        self.show_perf = show_perf
        self.frame_rate = frame_rate

        # Configure profiling based on show_perf flag
        if show_perf:
            enable_profiling()
            debug_print("Performance profiling enabled")
        else:
            disable_profiling()
            debug_print("Performance profiling disabled")

        # Gérer la fermeture propre
        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)

        # Timer pour traiter les signaux
        self.timer = QTimer()
        self.timer.timeout.connect(lambda: None)
        self.timer.start(100)

        # Timer pour afficher les statistiques de performance (si activé)
        self.perf_timer = None
        if self.show_perf:
            self.perf_timer = QTimer()
            self.perf_timer.timeout.connect(self.display_performance_stats)
            self.perf_timer.start(
                500
            )  # Affichage toutes les 500ms pour plus de réactivité

    def signal_handler(self, signum, frame):
        """Signal handler for clean shutdown"""
        debug_print("Shutdown signal received, cleaning up...")
        self.cleanup()
        self.app.quit()
        sys.exit(0)

    def display_performance_stats(self):
        """Display performance statistics using frame profiler"""
        if self.display_window and self.display_window.isVisible():
            try:
                # Update UI display with performance stats
                stats_text = get_stats_formatted()
                self.display_window.update_performance_display(stats_text)

                # Also print to console for debugging
                dump_stats()
            except Exception as e:
                debug_print(f"Performance monitoring error: {e}")

    def cleanup(self):
        """Resource cleanup"""
        debug_print("Cleaning up resources...")

        # Arrêter les timers
        if hasattr(self, "perf_timer") and self.perf_timer:
            self.perf_timer.stop()
        if hasattr(self, "timer") and self.timer:
            self.timer.stop()

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
        debug_print("Starting region selection process")

        # Check if auto use specific region is enabled
        if config.get("auto_use_specific_region"):
            debug_print("Auto use specific region is enabled")
            last_region = config.get("last_region")
            if last_region:
                x = int(last_region.get("x", 0))
                y = int(last_region.get("y", 0))
                width = int(last_region.get("width", 0))
                height = int(last_region.get("height", 0))
                debug_print(
                    f"Last region found: x={x}, y={y}, width={width}, height={height}"
                )
                if width > 0 and height > 0:
                    debug_print("Auto using specific region (skipping selector)")
                    self.on_selection_made(x, y, width, height, True)
                    return
                else:
                    debug_print(
                        "Last region has invalid dimensions, falling back to selection"
                    )

        # Normal selection process
        debug_print("Starting normal selection process")
        self.screen_selector = ScreenSelector()
        self.screen_selector.selection_made.connect(self.on_selection_made)
        self.screen_selector.show()

    def on_selection_made(self, x, y, width, height, reuse_last_region=False):
        """Callback called when a region is selected"""
        debug_print(f"Region selected: x={x}, y={y}, width={width}, height={height}")
        debug_print(f"Selection callback: reuse_last_region={reuse_last_region}")

        # Save last region if enabled (but not when reusing)
        if config.get("remember_last_region") and not reuse_last_region:
            debug_print("Saving last region to config")
            config.set(
                "last_region", {"x": x, "y": y, "width": width, "height": height}
            )
            config.save_settings()

        # Create display window with forced capture mode if specified
        debug_print(
            f"Creating display window with capture_mode={self.capture_mode}, frame_rate={self.frame_rate}"
        )
        self.display_window = DisplayWindow(
            x,
            y,
            width,
            height,
            capture_mode=self.capture_mode,
            frame_rate=self.frame_rate,
        )
        self.display_window.closed.connect(self.cleanup)
        self.display_window.show()

        # Configure performance display visibility
        show_perf = config.get("show_performance", False)
        self.display_window.set_performance_display_visible(self.show_perf or show_perf)

        # Give the window time to render before sending to background
        if config.get("auto_send_to_background"):
            # Use a timer to send to background after window is fully rendered
            QTimer.singleShot(200, self.display_window.send_to_background)
            debug_print("Will send to background in 200ms (auto mode)")
        elif reuse_last_region:
            # When reusing last region and auto_send disabled, bring window to front
            self.display_window.raise_()
            self.display_window.activateWindow()
            debug_print("Window brought to front (reused region)")

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
        "  region-to-share --config           # Open settings dialog only\n"
        "  region-to-share --mode portal-screencast  # Force Portal ScreenCast\n"
        "  region-to-share --mode mss         # Force MSS capture\n"
        "  region-to-share --frame-rate 60    # Set 60 FPS capture\n"
        "  region-to-share --perf --fps 15    # 15 FPS with performance monitoring\n"
        "  region-to-share --opacity 0.5 --auto-background  # Semi-transparent, auto-background\n"
        "  region-to-share --remember-region --auto-use-region  # Reuse last region automatically\n",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    parser.add_argument(
        "--mode",
        "--capture-mode",
        choices=["auto", "portal-screencast", "mss"],
        default="auto",
        help="Force a specific capture method (default: auto-detect)",
    )

    parser.add_argument("--debug", action="store_true", help="Enable debug output")

    parser.add_argument(
        "--config",
        action="store_true",
        help="Open configuration dialog only (don't start capture)",
    )

    parser.add_argument(
        "--perf",
        action="store_true",
        help="Enable performance monitoring and display FPS/timing stats",
    )

    parser.add_argument(
        "--frame-rate",
        "--fps",
        type=int,
        default=None,  # Will use config default if not specified
        help="Set maximum frame rate (FPS) for capture (default: from settings)",
    )

    parser.add_argument(
        "--opacity",
        type=float,
        default=None,
        help="Set window opacity (0.1-1.0, default: from settings)",
    )

    parser.add_argument(
        "--auto-background",
        action="store_true",
        help="Automatically send window to background after capture starts",
    )

    parser.add_argument(
        "--remember-region",
        action="store_true",
        help="Remember and offer to reuse the last selected region",
    )

    parser.add_argument(
        "--auto-use-region",
        action="store_true",
        help="Automatically use the last region without asking (requires --remember-region)",
    )

    parser.add_argument(
        "--version",
        action="version",
        version="Region-to-Share 1.0.6 - High-performance screen region capture for GNOME Wayland",
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

    # Handle --config argument: open settings dialog only
    if args.config:
        from PyQt5.QtWidgets import QApplication
        from region_to_share.settings_dialog import SettingsDialog

        app = QApplication(sys.argv + qt_args)
        dialog = SettingsDialog()
        if dialog.exec_() == SettingsDialog.Accepted:
            debug_print("Settings saved!")
        else:
            debug_print("Settings cancelled")
        return 0

    # Load configuration defaults
    default_frame_rate = int(config.get("frame_rate") or 30)
    default_capture_mode = str(config.get("capture_mode") or "auto")
    default_show_perf = bool(config.get("show_performance") or False)
    debug_print(
        f"Config defaults: frame_rate={default_frame_rate}, capture_mode={default_capture_mode}, show_perf={default_show_perf}"
    )

    # Use config defaults if not specified via command line
    if args.frame_rate is None:
        args.frame_rate = default_frame_rate
        debug_print(f"Using config frame rate: {args.frame_rate}")

    if args.mode == "auto" and default_capture_mode != "auto":
        args.mode = default_capture_mode
        debug_print(f"Using config capture mode: {args.mode}")

    if not args.perf and default_show_perf:
        args.perf = True
        debug_print("Using config performance monitoring setting")

    # Handle additional config overrides from command line
    # Opacity
    if args.opacity is not None:
        if 0.1 <= args.opacity <= 1.0:
            config.set("window_opacity", args.opacity)
            debug_print(f"Window opacity set to: {args.opacity}")
        else:
            debug_print(
                f"Invalid opacity value {args.opacity}, must be between 0.1 and 1.0"
            )

    # Auto background
    if args.auto_background:
        config.set("auto_send_to_background", True)
        debug_print("Auto background enabled")

    # Remember region
    if args.remember_region:
        config.set("remember_last_region", True)
        debug_print("Remember last region enabled")

    # Auto use region (requires remember region)
    if args.auto_use_region:
        if args.remember_region or config.get("remember_last_region"):
            config.set("auto_use_specific_region", True)
            config.set("remember_last_region", True)  # Ensure remember is also enabled
            debug_print("Auto use last region enabled")
        else:
            debug_print(
                "--auto-use-region requires --remember-region or remember_last_region in config"
            )

    # Set debug mode
    if args.debug:
        os.environ["REGION_TO_SHARE_DEBUG"] = "1"
        debug_print("Debug mode enabled")
        debug_print(f"Forced capture mode: {args.mode}")

    # Performance monitoring
    if args.perf:
        debug_print(f"Performance monitoring enabled : {args.perf}")
        # Initialize CPU monitoring
        init_cpu_monitoring()

    # Frame rate configuration
    if args.frame_rate != 30:
        debug_print(f"Frame rate set to {args.frame_rate} FPS")

    # Create app with specified capture mode
    capture_mode = None if args.mode == "auto" else args.mode
    app = RegionToShareApp(
        capture_mode=capture_mode, show_perf=args.perf, frame_rate=args.frame_rate
    )
    return app.run()


if __name__ == "__main__":
    sys.exit(main())
