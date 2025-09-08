# System Dependencies

This application requires certain system-level packages for full functionality.

## Current Application Features (v1.0.8)

### Core Features

- **Interactive screen region selection** with real-time preview
- **High-performance capture** (up to 240 FPS with optimization)
- **Multi-platform support**: Wayland (Portal/PipeWire) + X11 (MSS)
- **Global keyboard shortcuts** (GNOME/gsettings integration)
- **Advanced performance monitoring** with --perf flag
- **Auto-background mode** for streamlined workflow
- **Region memory** for quick reuse of last selected area
- **Portal session sharing** (single authorization on Wayland)

### Dependencies Overview

**Python Requirements** (see `requirements.txt`):

- PyQt5 >= 5.15.0 (GUI framework)
- mss >= 10.0.0 (X11 screen capture)
- dbus-python >= 1.3.0 (Wayland portal communication)
- psutil >= 5.9.0 (system monitoring)

**System Dependencies** (install via package manager):

- **PyGObject/GI** (GStreamer integration) - Available via pip OR system packages
- GStreamer + PipeWire plugin (Wayland capture)
- D-Bus (portal communication)
- Mesa/OpenGL (graphics acceleration)

## Installation by Platform

### PyGObject Installation Options

**Option 1: Via pip** (newer, cross-platform):

```bash
pip install PyGObject
```

**Option 2: Via system package manager** (traditional, more stable):

- **Ubuntu/Debian**: `sudo apt install python3-gi python3-gi-cairo`
- **Fedora/RHEL**: `sudo dnf install python3-gobject python3-gobject-devel`
- **Arch Linux**: `sudo pacman -S python-gobject`

**Note**: System packages often provide better integration and stability, especially for GStreamer bindings.

### Ubuntu/Debian (GNOME/Wayland)

```bash
sudo apt update
sudo apt install -y \
    python3-gi \
    python3-gi-cairo \
    gir1.2-gstreamer-1.0 \
    gir1.2-gst-plugins-base-1.0 \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-pipewire \
    pipewire \
    pipewire-pulse \
    libdbus-1-dev \
    pkg-config \
    python3-dev
```

### Fedora/RHEL

```bash
sudo dnf install -y \
    python3-gobject \
    python3-gobject-devel \
    gstreamer1-python \
    gstreamer1-plugins-good \
    gstreamer1-plugins-bad-free \
    gstreamer1-plugins-ugly-free \
    gstreamer1-plugin-pipewire \
    pipewire \
    pipewire-pulseaudio \
    dbus-devel \
    pkgconf-pkg-config \
    python3-devel
```

### Arch Linux

```bash
sudo pacman -S \
    python-gobject \
    gstreamer \
    gst-plugins-good \
    gst-plugins-bad \
    gst-plugins-ugly \
    gst-plugin-pipewire \
    pipewire \
    pipewire-pulse \
    dbus \
    pkgconf \
    python-devel
```

## Snap Package Dependencies

The snap package (recommended installation) includes all dependencies:

```bash
sudo snap install region-to-share
```

**Snap Architecture** (from `snapcraft.yaml`):

- **Base**: core22 (Ubuntu 22.04 LTS)
- **Confinement**: strict with essential plugs
- **Plugs**: desktop, wayland, x11, gsettings, process-control
- **Bundled**: PyQt5, GStreamer, PipeWire, all Python deps

## Feature-Specific Requirements

### Global Keyboard Shortcuts

- **GNOME**: `gsettings` + `gnome-settings-daemon`
- **KDE**: Not yet supported (future enhancement)
- **Other DEs**: Manual shortcut configuration required

### Wayland Screen Capture

- **Essential**: `gstreamer1.0-pipewire` plugin
- **Portal**: `xdg-desktop-portal` + implementation
- **Runtime**: PipeWire daemon (usually pre-installed)

### X11 Screen Capture

- **Fallback**: MSS library (pure Python, bundled)
- **Performance**: Hardware-accelerated when available

## Verification

After installing system dependencies, verify with:

```python
# Test core dependencies
import gi
gi.require_version('Gst', '1.0')
from gi.repository import Gst
import dbus
from PyQt5.QtWidgets import QApplication
import mss

print("✅ All system dependencies available")

# Test Wayland portal (if on Wayland)
try:
    bus = dbus.SessionBus()
    portal = bus.get_object('org.freedesktop.portal.Desktop',
                           '/org/freedesktop/portal/desktop')
    print("✅ Wayland portal available")
except:
    print("⚠️ Wayland portal not available (normal on X11)")
```

## Performance Notes

- **Wayland**: Optimal performance with hardware-accelerated PipeWire
- **X11**: MSS provides excellent performance with direct memory access
- **Snap**: Some performance overhead vs native installation
- **Frame rates**: 30-60 FPS recommended, 240 FPS maximum supported

## Troubleshooting

**Portal authorization fails**: Check PipeWire service status

**gsettings not found**: Install gnome-settings-daemon or use manual shortcuts

**High CPU usage**: Reduce frame rate or enable hardware acceleration

**Snap permissions**: Run with `--debug` flag to diagnose issues
