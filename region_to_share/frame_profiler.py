# frame_profiler.py
from time import perf_counter_ns
from collections import deque
from PyQt5.QtCore import Qt
from PyQt5.QtGui import QImage, QPixmap
import psutil
from region_to_share.debug import debug_print

NS = 1_000_000_000

# Global flag to enable/disable profiling
PROFILING_ENABLED = False


class FrameProfiler:
    def __init__(self, window=120):
        self.samples = deque(maxlen=window)

    def push(self, d):
        if PROFILING_ENABLED:
            self.samples.append(d)

    def stats(self):
        if not self.samples:
            return {}
        arr = list(self.samples)

        def pct(p):
            k = max(0, int(len(arr) * p) - 1)
            return sorted(arr)[k]

        avg = sum(arr) / len(arr)
        avg_ms = avg * 1000
        fps = min(1000 / avg_ms, 999.0) if avg_ms > 0 else 0.0

        return {
            "avg_ms": avg_ms,
            "p95_ms": pct(0.95) * 1000,
            "p99_ms": pct(0.99) * 1000,
            "fps": fps,
        }


prof_total = FrameProfiler()
prof_grab = FrameProfiler()
prof_qimg = FrameProfiler()
prof_qpx = FrameProfiler()
prof_paint = FrameProfiler()

# CPU monitoring initialization
_cpu_initialized = False


def init_cpu_monitoring():
    """Initialize CPU monitoring with warm-up"""
    global _cpu_initialized
    if not _cpu_initialized:
        # Warm-up call to initialize global CPU monitoring
        psutil.cpu_percent(interval=None)
        _cpu_initialized = True


def get_cpu_percent():
    """Get current system CPU percentage"""
    global _cpu_initialized
    if not _cpu_initialized:
        init_cpu_monitoring()
        return 0.0
    # Use global CPU percentage (non-blocking)
    return psutil.cpu_percent(interval=None)


def now():
    if not PROFILING_ENABLED:
        return 0
    return perf_counter_ns() / NS


# Compat flag
try:
    NO_CONV = Qt.ImageConversionFlag.NoFormatConversion  # PyQt6
except AttributeError:
    NO_CONV = Qt.NoFormatConversion  # PyQt5


def capture_frame(self, x, y, w, h, painter=None):
    t0 = now()
    s = self._sct.grab({"top": y, "left": x, "width": w, "height": h})
    t1 = now()

    # buffer persistant (pas de numpy/tobytes)
    self._qimage_buf = bytearray(s.bgra)

    fmt = (
        QImage.Format_BGRA8888  # type: ignore
        if hasattr(QImage, "Format_BGRA8888")
        else QImage.Format_ARGB32
    )
    img = QImage(self._qimage_buf, s.width, s.height, s.width * 4, fmt)
    t2 = now()

    # Réutiliser un QPixmap
    if not hasattr(self, "_px"):
        self._px = QPixmap()
    self._px.convertFromImage(img, NO_CONV)
    t3 = now()

    if painter:
        painter.drawPixmap(0, 0, self._px)
    t4 = now()

    prof_total.push(t4 - t0)
    prof_grab.push(t1 - t0)
    prof_qimg.push(t2 - t1)
    prof_qpx.push(t3 - t2)
    prof_paint.push(t4 - t3)


def enable_profiling():
    """Enable performance profiling"""
    global PROFILING_ENABLED
    PROFILING_ENABLED = True


def disable_profiling():
    """Disable performance profiling"""
    global PROFILING_ENABLED
    PROFILING_ENABLED = False


def is_profiling_enabled():
    """Check if profiling is enabled"""
    return PROFILING_ENABLED


def get_stats_formatted():
    """Return formatted performance statistics as a string"""
    if not PROFILING_ENABLED:
        return ""

    cpu = get_cpu_percent()
    return (
        f"FPS {prof_total.stats().get('fps',0):.1f} | "
        f"tot avg {prof_total.stats().get('avg_ms',0):.2f} ms (p95 {prof_total.stats().get('p95_ms',0):.2f}) | "
        f"grab {prof_grab.stats().get('avg_ms',0):.2f} | "
        f"QImage {prof_qimg.stats().get('avg_ms',0):.2f} | "
        f"QPixmap {prof_qpx.stats().get('avg_ms',0):.2f} | "
        f"paint {prof_paint.stats().get('avg_ms',0):.2f} | "
        f"CPU {cpu:.0f}%"
    )


def dump_stats():
    """Print performance statistics to console"""
    if PROFILING_ENABLED:
        debug_print(get_stats_formatted())
