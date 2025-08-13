"""
Version simplifiée et optimisée avec capture en mémoire
"""

import os
import tempfile
import time
import subprocess
import shutil
from typing import Optional, Tuple
from PyQt5.QtGui import QPixmap

try:
    import dbus
    import dbus.mainloop.glib

    HAS_DBUS = True
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
except ImportError:
    HAS_DBUS = False


class MutterScreenCastFast:
    """
    Version ultra-rapide avec capture directe en mémoire
    """

    def __init__(self):
        self.bus = None
        self.session_proxy = None
        self.session_interface = None
        self.current_stream = None
        self.is_session_active = False
        self.capture_region = None
        self.pipewire_node_id = None
        self.stream_ready = False
        self.temp_files = []
        self.frame_counter = 0

        # Multi-file cache for extreme performance (JPEG optimized)
        self.capture_files = [
            tempfile.mktemp(suffix=".jpg"),
            tempfile.mktemp(suffix=".jpg"),
            tempfile.mktemp(suffix=".jpg"),
        ]
        self.current_file_index = 0
        self.background_process = None
        self.persistent_pipeline = None
        self.last_capture_time = 0
        self.start_time = None

        if HAS_DBUS:
            self.bus = dbus.SessionBus()

    def is_available(self) -> bool:
        """Check if Mutter ScreenCast is available"""
        if not HAS_DBUS or not self.bus:
            return False
        try:
            proxy = self.bus.get_object(
                "org.gnome.Mutter.ScreenCast", "/org/gnome/Mutter/ScreenCast"
            )
            interface = dbus.Interface(proxy, "org.gnome.Mutter.ScreenCast")
            test_session = interface.CreateSession({})
            return True
        except Exception:
            return False

    def initialize_session(self) -> bool:
        """Initialize a persistent ScreenCast session"""
        if self.is_session_active:
            return True
        if not self.is_available() or not self.bus:
            return False
        try:
            screencast_proxy = self.bus.get_object(
                "org.gnome.Mutter.ScreenCast", "/org/gnome/Mutter/ScreenCast"
            )
            screencast_interface = dbus.Interface(
                screencast_proxy, "org.gnome.Mutter.ScreenCast"
            )
            session_path = screencast_interface.CreateSession({})
            self.session_proxy = self.bus.get_object(
                "org.gnome.Mutter.ScreenCast", session_path
            )
            self.session_interface = dbus.Interface(
                self.session_proxy, "org.gnome.Mutter.ScreenCast.Session"
            )
            print("✅ Mutter ScreenCast session initialized")
            return True
        except Exception as e:
            print(f"❌ Failed to initialize session: {e}")
            return False

    def start_area_capture(self, x: int, y: int, width: int, height: int) -> bool:
        """Start capturing a specific area"""
        if not self.initialize_session():
            return False
        try:
            self.stop_capture()
            properties = {
                "cursor-mode": dbus.UInt32(1),
                "is-recording": dbus.Boolean(False),
            }
            stream_path = self.session_interface.RecordArea(
                dbus.Int32(x),
                dbus.Int32(y),
                dbus.Int32(width),
                dbus.Int32(height),
                properties,
            )
            self.session_interface.Start()
            self.current_stream = stream_path
            self.is_session_active = True
            self.capture_region = (x, y, width, height)
            self._setup_pipewire_signal_handler(stream_path)
            print(f"✅ Area capture started: {width}x{height} at ({x},{y})")
            return True
        except Exception as e:
            print(f"❌ Failed to start area capture: {e}")
            return False

    def _setup_pipewire_signal_handler(self, stream_path):
        """Set up signal handler for PipeWire stream"""
        try:
            stream_proxy = self.bus.get_object(
                "org.gnome.Mutter.ScreenCast", stream_path
            )

            def on_pipewire_stream_added(node_id):
                print(f"🎵 PipeWire stream ready, node ID: {node_id}")
                self.pipewire_node_id = int(node_id)
                self.stream_ready = True
                self.start_time = time.time()  # Initialize timer for FPS calculation

            stream_proxy.connect_to_signal(
                "PipeWireStreamAdded",
                on_pipewire_stream_added,
                dbus_interface="org.gnome.Mutter.ScreenCast.Stream",
            )
        except Exception as e:
            print(f"⚠️  Could not set up PipeWire signal handler: {e}")

    def capture_frame(self) -> Optional[QPixmap]:
        """High-speed frame capture"""
        if not self.is_session_active or not self.current_stream:
            return None
        return self.capture_frame_pipewire_fast()

    def capture_frame_pipewire_fast(self) -> Optional[QPixmap]:
        """
        ULTIMATE SPEED: Single-shot optimized for 20+ FPS
        """
        if not self.capture_region or not self.current_stream:
            return None

        if not self.stream_ready:
            time.sleep(0.005)  # 5ms minimal wait
            if not self.stream_ready:
                return None

        try:
            node_id = self.pipewire_node_id or 0

            # Rotate capture files for parallel processing
            current_file = self.capture_files[self.current_file_index]
            self.current_file_index = (self.current_file_index + 1) % len(
                self.capture_files
            )

            # ULTIMATE optimization: absolute minimal timeout
            # Detect if running in snap environment
            python_cmd = "/usr/bin/python3"
            if os.environ.get("SNAP"):
                python_cmd = "python3"  # Use PATH resolution in snap

            print(
                f"🔍 DEBUG: python_cmd={python_cmd}, SNAP={bool(os.environ.get('SNAP'))}"
            )
            print(f"🔍 DEBUG: node_id={node_id}, current_file={current_file}")

            # Test direct GStreamer sans subprocess pour debug
            print(f"🔍 DEBUG: Tentative capture directe sans subprocess")

            try:
                import gi

                gi.require_version("Gst", "1.0")
                from gi.repository import Gst

                # Initialiser GStreamer dans le processus principal
                if not hasattr(self, "_gst_initialized"):
                    Gst.init(None)
                    self._gst_initialized = True
                    print(f"🔍 DEBUG: GStreamer initialisé dans le processus principal")

                # Pipeline simple et direct
                pipeline_str = f"pipewiresrc path={node_id} num-buffers=1 do-timestamp=true ! queue max-size-buffers=1 leaky=downstream ! videorate skip-to-first=true ! videoconvert ! jpegenc quality=50 ! filesink location={current_file}"
                print(f"🔍 DEBUG: Pipeline direct: {pipeline_str[:100]}...")

                pipeline = Gst.parse_launch(pipeline_str)
                bus = pipeline.get_bus()

                ret = pipeline.set_state(Gst.State.PLAYING)
                print(f"🔍 DEBUG: État PLAYING défini: {ret}")

                # Attendre le message avec timeout plus long
                msg = bus.timed_pop_filtered(
                    300 * Gst.MSECOND,  # 300ms
                    Gst.MessageType.ERROR | Gst.MessageType.EOS,
                )

                if msg:
                    print(f"🔍 DEBUG: Message reçu: {msg.type}")
                    if msg.type == Gst.MessageType.ERROR:
                        error, debug = msg.parse_error()
                        print(f"🔍 DEBUG: ERREUR GStreamer: {error}")
                        print(f"🔍 DEBUG: Info debug: {debug}")
                else:
                    print(f"🔍 DEBUG: Timeout - aucun message")

                pipeline.set_state(Gst.State.NULL)

            except Exception as e:
                print(f"🔍 DEBUG: Exception capture directe: {e}")
                # Fallback au subprocess original si échec
                pass

            # Puis essayer le subprocess comme avant
            gst_command = [
                python_cmd,
                "-c",
                f'''
import gi
gi.require_version("Gst", "1.0")
from gi.repository import Gst
import sys

print("🔍 GStreamer DEBUG: Starting", file=sys.stderr, flush=True)
Gst.init(None)
print("🔍 GStreamer DEBUG: Initialized", file=sys.stderr, flush=True)

# BALANCED SPEED pipeline with optimized JPEG
pipeline_str = """
    pipewiresrc path={node_id} num-buffers=1 do-timestamp=true ! 
    queue max-size-buffers=1 leaky=downstream ! 
    videorate skip-to-first=true ! 
    videoconvert ! 
    jpegenc quality=50 idct-method=1 ! 
    filesink location={current_file}
"""

print(f"🔍 GStreamer DEBUG: Pipeline: {{pipeline_str.strip()}}", file=sys.stderr, flush=True)

pipeline = Gst.parse_launch(pipeline_str.strip())
bus = pipeline.get_bus()

print("🔍 GStreamer DEBUG: Setting to PLAYING", file=sys.stderr, flush=True)
ret = pipeline.set_state(Gst.State.PLAYING)
if ret == Gst.StateChangeReturn.FAILURE:
    print("🔍 GStreamer DEBUG: FAILED to set PLAYING", file=sys.stderr, flush=True)
    sys.exit(1)
else:
    print(f"🔍 GStreamer DEBUG: Set PLAYING result: {{ret}}", file=sys.stderr, flush=True)

# BALANCED SPEED: Fast but stable timeout
print("🔍 GStreamer DEBUG: Waiting for message...", file=sys.stderr, flush=True)
msg = bus.timed_pop_filtered(
    120 * Gst.MSECOND,  # 120ms - sweet spot
    Gst.MessageType.ERROR | Gst.MessageType.EOS
)

if msg:
    print(f"🔍 GStreamer DEBUG: Message type: {{msg.type}}", file=sys.stderr, flush=True)
    if msg.type == Gst.MessageType.ERROR:
        error, debug = msg.parse_error()
        print(f"🔍 GStreamer DEBUG: ERROR: {{error}}, DEBUG: {{debug}}", file=sys.stderr, flush=True)
else:
    print("🔍 GStreamer DEBUG: No message (timeout)", file=sys.stderr, flush=True)

print("🔍 GStreamer DEBUG: Setting to NULL", file=sys.stderr, flush=True)
pipeline.set_state(Gst.State.NULL)
sys.exit(0 if not msg or msg.type == Gst.MessageType.EOS else 1)
''',
            ]

            print(f"🔍 DEBUG: Running command: {gst_command[0]} -c [gstreamer_script]")

            # Préparer l'environnement pour le subprocess dans le snap
            subprocess_env = os.environ.copy()
            if os.environ.get("SNAP"):
                snap_path = os.environ.get("SNAP", "")
                if snap_path:
                    # Configuration complète GStreamer pour subprocess
                    subprocess_env.update(
                        {
                            "PYTHONPATH": f"{snap_path}/lib/python3.10/site-packages:{snap_path}/usr/lib/python3/dist-packages",
                            "GI_TYPELIB_PATH": f"{snap_path}/usr/lib/x86_64-linux-gnu/girepository-1.0:{snap_path}/usr/lib/girepository-1.0",
                            "GST_PLUGIN_PATH": f"{snap_path}/usr/lib/x86_64-linux-gnu/gstreamer-1.0",
                            "GST_PLUGIN_SYSTEM_PATH": f"{snap_path}/usr/lib/x86_64-linux-gnu/gstreamer-1.0",
                            "LD_LIBRARY_PATH": f"{snap_path}/usr/lib/x86_64-linux-gnu:{snap_path}/lib/x86_64-linux-gnu",
                            "PKG_CONFIG_PATH": f"{snap_path}/usr/lib/x86_64-linux-gnu/pkgconfig",
                            # Force flush stderr for debugging
                            "PYTHONUNBUFFERED": "1",
                        }
                    )
                    print(f"🔍 DEBUG: Snap environment configured for subprocess")

            # BALANCED subprocess timeout
            result = subprocess.run(
                gst_command,
                capture_output=True,
                timeout=0.25,  # 250ms - stable
                text=True,
                env=subprocess_env,
            )

            print(f"🔍 DEBUG: Return code: {result.returncode}")
            if result.stderr and result.stderr.strip():
                print(f"🔍 DEBUG: STDERR: {result.stderr}")
            if result.stdout and result.stdout.strip():
                print(f"🔍 DEBUG: STDOUT: {result.stdout}")

            # Check for GStreamer-specific errors in stderr
            if result.stderr and (
                "ERROR" in result.stderr or "WARNING" in result.stderr
            ):
                print(f"⚠️  GStreamer issues detected: {result.stderr}")

            if result.returncode == 0 and os.path.exists(current_file):
                file_size = os.path.getsize(current_file)
                print(f"🔍 DEBUG: File created: {current_file}, size: {file_size}")
                if file_size > 30:  # Ultra-minimal size check
                    print(f"🔍 DEBUG: Loading QPixmap from {current_file}")
                    pixmap = QPixmap(current_file)
                    print(
                        f"🔍 DEBUG: QPixmap loaded, isNull: {pixmap.isNull()}, size: {pixmap.size()}"
                    )
                    if not pixmap.isNull():
                        self.frame_counter += 1
                        # Log every 200 frames for 30+ FPS
                        if self.frame_counter % 200 == 0:
                            if self.start_time:
                                elapsed = time.time() - self.start_time
                                fps_estimate = (
                                    self.frame_counter / elapsed if elapsed > 0 else 0
                                )
                            else:
                                fps_estimate = 0
                            print(
                                f"🔥 ULTIMATE frame {self.frame_counter}, ~{fps_estimate:.1f} FPS, size: {file_size}"
                            )
                        return pixmap
                    else:
                        print(f"🔍 DEBUG: QPixmap is NULL!")
                else:
                    print(f"🔍 DEBUG: File too small: {file_size} bytes")
            else:
                print(
                    f"🔍 DEBUG: Return code: {result.returncode}, file exists: {os.path.exists(current_file)}"
                )

            return None

        except Exception:
            return None

    def stop_capture(self):
        """Stop the current capture"""
        if self.is_session_active and self.session_interface:
            try:
                self.session_interface.Stop()
                self.is_session_active = False
                self.current_stream = None
                self.capture_region = None
                print("✅ Capture stopped")
            except Exception as e:
                print(f"❌ Error stopping capture: {e}")

    def cleanup(self):
        """Clean up resources"""
        self.stop_capture()

        # Clean up capture files (multiple files)
        for capture_file in self.capture_files:
            try:
                if os.path.exists(capture_file):
                    os.remove(capture_file)
            except:
                pass

        # Clean up temp files
        for temp_file in self.temp_files:
            try:
                if os.path.exists(temp_file):
                    os.remove(temp_file)
            except:
                pass
        self.temp_files.clear()

        # Stop persistent pipeline
        if self.persistent_pipeline and self.persistent_pipeline.poll() is None:
            try:
                self.persistent_pipeline.terminate()
                self.persistent_pipeline.wait(timeout=2)
                print("✅ Persistent pipeline stopped")
            except:
                try:
                    self.persistent_pipeline.kill()
                except:
                    pass

        # Clean up persistent pipeline temp files
        try:
            for i in range(10):  # Clean up potential temp files
                temp_file = f"/tmp/pipewire_frame_{i:03d}.jpg"
                if os.path.exists(temp_file):
                    os.remove(temp_file)
        except:
            pass

        # Stop background process if any
        if self.background_process and self.background_process.poll() is None:
            try:
                self.background_process.terminate()
                self.background_process.wait(timeout=1)
            except:
                pass

        # Reset state
        self.session_proxy = None
        self.session_interface = None
        self.pipewire_node_id = None
        self.stream_ready = False
        self.frame_counter = 0
        self.background_process = None
        self.persistent_pipeline = None
        self.start_time = None

    def __del__(self):
        """Cleanup on destruction"""
        self.cleanup()


# Compatibility
MutterScreenCastOptimized = MutterScreenCastFast
MutterScreenCast = MutterScreenCastFast


def create_mutter_capture():
    return MutterScreenCastFast()


def test_mutter_availability():
    capture = create_mutter_capture()
    return capture.is_available()
