# portal_screencast.py
import gi

# Spécifier les versions avant l'import
gi.require_version("Gst", "1.0")
gi.require_version("GstApp", "1.0")

import dbus
import dbus.mainloop.glib
import time
from gi.repository import Gst  # type:ignore
from PyQt5.QtGui import QImage, QPixmap
from PyQt5.QtCore import QObject, pyqtSignal, QTimer, Qt
from PyQt5.QtWidgets import QApplication
from region_to_share.debug import debug_print

# Import NO_CONV from frame_profiler for optimization
try:
    from .frame_profiler import NO_CONV
except ImportError:
    try:
        NO_CONV = Qt.ImageConversionFlag.NoFormatConversion  # PyQt6
    except AttributeError:
        NO_CONV = Qt.NoFormatConversion  # PyQt5

dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)


class PortalScreenCast(QObject):
    """ScreenCast via portail XDG pour compatibilité snap strict sous Wayland"""

    session_ready = pyqtSignal()
    error_occurred = pyqtSignal(str)

    def __init__(self):
        super().__init__()
        self.bus = None
        self.portal = None
        self.iface = None
        self.session_handle = None
        self.pw_fd = None
        self.node_id = None
        self._gst_inited = False
        self._pipeline = None
        self._appsink = None
        self._region = None  # (x,y,w,h)
        self._loop = None
        self._loop_thread = None
        self._session_initialized = False

        # Variables pour l'attente Qt des signaux D-Bus
        self._waiting_timer = None
        self._timeout_timer = None

    def _init_dbus(self):
        """Initialize D-Bus connection"""
        try:
            self.bus = dbus.SessionBus()
            self.portal = self.bus.get_object(
                "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop"
            )
            self.iface = dbus.Interface(
                self.portal, "org.freedesktop.portal.ScreenCast"
            )
            return True
        except Exception as e:
            debug_print(f"Error initializing D-Bus: {e}")
            return False

    def _init_gst(self):
        """Initialise GStreamer avec gestion d'erreur robuste"""
        if not self._gst_inited:
            try:
                Gst.init(None)
                self._gst_inited = True
                debug_print("✅ GStreamer initialized successfully")

                # Test de disponibilité PipeWire
                self._test_pipewire_availability()

            except Exception as e:
                debug_print(f"❌ Failed to initialize GStreamer: {e}")
                raise RuntimeError(f"GStreamer initialization failed: {e}")

    def _test_pipewire_availability(self):
        """Test si PipeWire est disponible et fonctionnel"""
        try:
            registry = Gst.Registry.get()
            pipewiresrc_feature = registry.find_feature(
                "pipewiresrc", Gst.ElementFactory
            )
            if not pipewiresrc_feature:
                raise RuntimeError("PipeWire GStreamer plugin not found")

            # Test de création d'un élément pipewiresrc
            test_element = Gst.ElementFactory.make("pipewiresrc", "test")
            if not test_element:
                raise RuntimeError("Cannot create pipewiresrc element")

            debug_print("✅ PipeWire GStreamer plugin available")

        except Exception as e:
            debug_print(f"❌ PipeWire test failed: {e}")
            raise RuntimeError(f"PipeWire not available: {e}")

    def _start_glib_loop(self):
        """Démarre la boucle GLib (ne plus nécessaire avec l'approche Qt)"""
        # Plus besoin de thread GLib séparé avec l'approche Qt
        pass

    def _wait_for_response_qt(self, check_function, timeout_ms=5000):
        """Attendre une réponse en utilisant QTimer (compatible Qt)"""
        if self._waiting_timer is not None:
            self._waiting_timer.stop()
            self._waiting_timer = None
        if self._timeout_timer is not None:
            self._timeout_timer.stop()
            self._timeout_timer = None

        self._wait_result = None
        self._wait_timeout = False

        # Timer pour vérifier périodiquement la condition
        self._waiting_timer = QTimer()
        self._waiting_timer.timeout.connect(
            lambda: self._check_wait_condition(check_function)
        )
        self._waiting_timer.start(10)  # Vérifier toutes les 10ms

        # Timer pour timeout
        self._timeout_timer = QTimer()
        self._timeout_timer.timeout.connect(self._on_wait_timeout)
        self._timeout_timer.setSingleShot(True)
        self._timeout_timer.start(timeout_ms)

        # Attente synchrone avec processEvents Qt
        start_time = time.time()
        while (
            self._wait_result is None
            and not self._wait_timeout
            and (time.time() - start_time) * 1000 < timeout_ms
        ):
            QApplication.processEvents()
            time.sleep(0.001)  # Très petite pause

        # Nettoyage
        if self._waiting_timer:
            self._waiting_timer.stop()
            self._waiting_timer = None
        if self._timeout_timer:
            self._timeout_timer.stop()
            self._timeout_timer = None

        return self._wait_result

    def _check_wait_condition(self, check_function):
        """Vérifier la condition d'attente"""
        result = check_function()
        if result is not None:
            self._wait_result = result
            if self._waiting_timer:
                self._waiting_timer.stop()

    def _on_wait_timeout(self):
        """Callback timeout"""
        self._wait_timeout = True
        if self._timeout_timer:
            self._timeout_timer.stop()

    def initialize_session(self):
        """Initialize a screen capture session via the portal"""
        if self._session_initialized:
            return True

        if not self._init_dbus():
            return False

        # Assert bus and iface are initialized
        if self.bus is None:
            raise RuntimeError("D-Bus session bus is not initialized")
        if self.iface is None:
            raise RuntimeError("D-Bus interface is not initialized")

        self._start_glib_loop()

        try:
            import random
            import string

            session_token = "".join(
                random.choices(string.ascii_letters + string.digits, k=16)
            )

            session_options = {
                "handle_token": session_token,
                "session_handle_token": session_token,
            }

            sender_name = self.bus.get_unique_name()[1:].replace(".", "_")
            request_path = (
                f"/org/freedesktop/portal/desktop/request/{sender_name}/{session_token}"
            )

            self._session_response_received = False
            self._session_handle_received = None

            def on_session_response(response, results):
                debug_print(f"Session Response: response={response}, results={results}")
                if response == 0:
                    self._session_handle_received = results.get("session_handle")
                    debug_print(
                        f"Session handle received: {self._session_handle_received}"
                    )
                else:
                    debug_print(f"Session error: {response}")
                self._session_response_received = True

            try:
                request_obj = self.bus.get_object(
                    "org.freedesktop.portal.Desktop", request_path
                )
                request_iface = dbus.Interface(
                    request_obj, "org.freedesktop.portal.Request"
                )
                request_iface.connect_to_signal("Response", on_session_response)
                debug_print(f"Signal connected on: {request_path}")
            except Exception as e:
                debug_print(f"Unable to connect signal: {e}")

            session_result = self.iface.CreateSession(session_options)
            debug_print(f"CreateSession result: {session_result}")

            def check_session_response():
                return (
                    self._session_handle_received
                    if self._session_response_received
                    else None
                )

            result = self._wait_for_response_qt(check_session_response, timeout_ms=5000)

            if result is None:
                raise RuntimeError("Timeout waiting for session response")

            self.session_handle = result
            assert (
                self.session_handle is not None
            ), "Session handle is None after CreateSession"

            select_token = "".join(
                random.choices(string.ascii_letters + string.digits, k=16)
            )
            select_options = {
                "types": dbus.UInt32(1),
                "multiple": dbus.Boolean(False),
                "cursor_mode": dbus.UInt32(2),
                "handle_token": select_token,
            }

            sender_name = self.bus.get_unique_name()[1:].replace(".", "_")
            select_request_path = (
                f"/org/freedesktop/portal/desktop/request/{sender_name}/{select_token}"
            )
            self._select_response_received = False

            def on_select_response(response, results):
                debug_print(f"Select Response: response={response}, results={results}")
                self._select_response_received = True

            try:
                select_request_obj = self.bus.get_object(
                    "org.freedesktop.portal.Desktop", select_request_path
                )
                select_request_iface = dbus.Interface(
                    select_request_obj, "org.freedesktop.portal.Request"
                )
                select_request_iface.connect_to_signal("Response", on_select_response)
            except Exception as e:
                debug_print(f"Unable to connect select signal: {e}")

            select_result = self.iface.SelectSources(
                self.session_handle, select_options
            )
            debug_print(f"SelectSources result: {select_result}")

            def check_select_response():
                return True if self._select_response_received else None

            result = self._wait_for_response_qt(check_select_response, timeout_ms=10000)

            if result is None:
                raise RuntimeError("Timeout waiting for source selection response")

            self._session_initialized = True
            debug_print("✅ Portal session initialized")
            return True

        except Exception as e:
            debug_print(f"❌ Error initializing portal session: {e}")
            import traceback

            traceback.print_exc()
            return False

    def start_area_capture(self, x, y, w, h):
        """Start capturing a specific area"""
        if not self._session_initialized:
            if not self.initialize_session():
                return False

        # Assert bus and iface are initialized
        if self.bus is None:
            raise RuntimeError("D-Bus session bus is not initialized")
        if self.iface is None:
            raise RuntimeError("D-Bus interface is not initialized")

        try:
            import random
            import string

            start_token = "".join(
                random.choices(string.ascii_letters + string.digits, k=16)
            )
            start_options = {"handle_token": start_token}

            sender_name = self.bus.get_unique_name()[1:].replace(".", "_")
            start_request_path = (
                f"/org/freedesktop/portal/desktop/request/{sender_name}/{start_token}"
            )
            self._start_response_received = False
            self._streams_received = None

            def on_start_response(response, results):
                debug_print(f"Start Response: response={response}, results={results}")
                if response == 0:
                    streams = results.get("streams", [])
                    self._streams_received = streams
                    debug_print(f"Streams received: {streams}")
                    if streams:
                        self.node_id = int(streams[0][0])
                        debug_print(f"Node ID: {self.node_id}")
                else:
                    debug_print(f"Start error: {response}")
                self._start_response_received = True

            try:
                start_request_obj = self.bus.get_object(
                    "org.freedesktop.portal.Desktop", start_request_path
                )
                start_request_iface = dbus.Interface(
                    start_request_obj, "org.freedesktop.portal.Request"
                )
                start_request_iface.connect_to_signal("Response", on_start_response)
                debug_print(f"Start signal connected on: {start_request_path}")
            except Exception as e:
                debug_print(f"Unable to connect start signal: {e}")

            parent_window = ""
            start_result = self.iface.Start(
                self.session_handle, parent_window, start_options
            )
            debug_print(f"Start result: {start_result}")

            def check_start_response():
                return True if self._start_response_received else None

            result = self._wait_for_response_qt(check_start_response, timeout_ms=10000)

            if result is None:
                raise RuntimeError("Timeout waiting for Start response")

            if not self._streams_received:
                raise RuntimeError("No stream received after Start")

            self.pw_fd = self.iface.OpenPipeWireRemote(self.session_handle, {})
            debug_print(f"PipeWire FD: {self.pw_fd}")

            if not self.pw_fd:
                raise RuntimeError("Unable to open PipeWire remote")

            self._region = (x, y, w, h)
            success = self._build_pipeline()

            if success:
                debug_print(f"✅ Portal capture started for region {x},{y} {w}x{h}")

            return success

        except Exception as e:
            debug_print(f"❌ Error starting portal capture: {e}")
            import traceback

            traceback.print_exc()
            return False

    def _build_pipeline(self):
        """Build the GStreamer pipeline with integrated crop"""
        try:
            self._init_gst()

            if not self._region:
                raise ValueError("Region is None when building pipeline")
            if self.pw_fd is None:
                raise RuntimeError("PipeWire FD is None when building pipeline")
            if self.node_id is None:
                raise RuntimeError("Node ID is None when building pipeline")

            x, y, w, h = self._region

            pw_fd_int = self.pw_fd if isinstance(self.pw_fd, int) else self.pw_fd.take()

            pipeline_str = f"""
                pipewiresrc fd={pw_fd_int} path={self.node_id} always-copy=true ! 
                videoconvert ! 
                video/x-raw,format=BGRx ! 
                appsink name=sink emit-signals=true max-buffers=2 drop=true sync=false
            """

            debug_print(f"GStreamer pipeline: {pipeline_str}")

            # Vérification de la disponibilité du plugin pipewiresrc
            registry = Gst.Registry.get()
            pipewiresrc_feature = registry.find_feature(
                "pipewiresrc", Gst.ElementFactory
            )
            if not pipewiresrc_feature:
                raise RuntimeError("PipeWire GStreamer plugin not available")

            try:
                self._pipeline = Gst.parse_launch(pipeline_str)
                if not self._pipeline:
                    raise RuntimeError("Unable to create GStreamer pipeline")
            except Exception as e:
                debug_print(f"❌ Pipeline creation failed: {e}")
                raise RuntimeError(f"GStreamer pipeline creation failed: {e}")

            self._appsink = self._pipeline.get_child_by_name("sink")
            if not self._appsink:
                raise RuntimeError("Unable to get appsink from pipeline")

            ret = self._pipeline.set_state(Gst.State.PLAYING)
            if ret == Gst.StateChangeReturn.FAILURE:
                debug_print("❌ Unable to start GStreamer pipeline")
                bus = self._pipeline.get_bus()
                while True:
                    msg = bus.pop()
                    if not msg:
                        break
                    if msg.type == Gst.MessageType.ERROR:
                        err, debug = msg.parse_error()
                        debug_print(f"GStreamer ERROR: {err}")
                        debug_print(f"GStreamer DEBUG: {debug}")
                return False

            debug_print("✅ GStreamer pipeline started")
            return True

        except Exception as e:
            debug_print(f"❌ Error building pipeline: {e}")
            return False

    def capture_frame(self) -> QPixmap:
        """Capture a frame and return a cropped QPixmap"""
        if self._appsink is None:
            raise RuntimeError("Appsink is None when capturing frame")
        if self._region is None:
            raise RuntimeError("Region is None when capturing frame")

        try:
            sample = self._appsink.emit("try-pull-sample", 150_000_000)
            if not sample:
                raise RuntimeError("No sample received from appsink")

            buf = sample.get_buffer()
            caps = sample.get_caps().get_structure(0)

            stream_w = caps.get_value("width")
            stream_h = caps.get_value("height")

            if not stream_w or not stream_h:
                raise RuntimeError("Unable to get stream dimensions")

            success, mapinfo = buf.map(Gst.MapFlags.READ)
            if not success:
                raise RuntimeError("Unable to map buffer for reading")

            try:
                bytes_per_line = stream_w * 4
                full_image = QImage(
                    mapinfo.data,
                    stream_w,
                    stream_h,
                    bytes_per_line,
                    QImage.Format_RGB32,
                )

                x, y, w, h = self._region

                if x < 0:
                    x = 0
                if y < 0:
                    y = 0
                if x + w > stream_w:
                    w = stream_w - x
                if y + h > stream_h:
                    h = stream_h - y

                if w <= 0 or h <= 0:
                    raise RuntimeError("Invalid crop region for QPixmap")

                # Version sécurisée : utiliser copy() mais optimiser la conversion QPixmap
                cropped = full_image.copy(x, y, w, h)

                # Optimisation: utiliser fromImage avec NO_CONV (seulement ça)
                return QPixmap.fromImage(cropped, NO_CONV)

            finally:
                buf.unmap(mapinfo)

        except Exception as e:
            debug_print(f"❌ Error capturing frame: {e}")
            raise

    def cleanup(self):
        """Clean up resources"""
        try:
            # Clean up optimized pixmap
            if hasattr(self, "_portal_pixmap"):
                delattr(self, "_portal_pixmap")

            if self._pipeline:
                self._pipeline.set_state(Gst.State.NULL)
                self._pipeline = None
                self._appsink = None

            if self.session_handle and self.iface:
                try:
                    # Close portal session if needed
                    pass
                except Exception:
                    pass

            if self._loop:
                self._loop.quit()
                self._loop = None

            self._session_initialized = False
            debug_print("🧹 Portal cleanup done")

        except Exception as e:
            debug_print(f"Error cleaning up portal: {e}")

    def __del__(self):
        self.cleanup()
