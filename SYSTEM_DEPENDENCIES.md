# System Dependencies

This application requires certain system-level packages for full functionality.

## GNOME/Wayland Systems (Ubuntu/Debian)

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
    pipewire \
    pipewire-pulse \
    libdbus-1-dev \
    pkg-config \
    python3-dev
```

## Fedora/RHEL

```bash
sudo dnf install -y \
    python3-gobject \
    python3-gobject-devel \
    gstreamer1-python \
    gstreamer1-plugins-good \
    gstreamer1-plugins-bad-free \
    gstreamer1-plugins-ugly-free \
    pipewire \
    pipewire-pulseaudio \
    dbus-devel \
    pkgconf-pkg-config \
    python3-devel
```

## Arch Linux

```bash
sudo pacman -S \
    python-gobject \
    gstreamer \
    gst-plugins-good \
    gst-plugins-bad \
    gst-plugins-ugly \
    pipewire \
    pipewire-pulse \
    dbus \
    pkgconf \
    python-devel
```

## Notes

- **PyGObject/GI**: Required for GStreamer integration but not available via pip
- **PipeWire**: Essential for Wayland screen capture on modern systems
- **D-Bus**: System message bus for XDG Portal ScreenCast API
- **GStreamer**: Media framework for video processing

## Verification

After installing system dependencies, verify with:

```python
import gi
gi.require_version('Gst', '1.0')
from gi.repository import Gst
import dbus
print("✅ All system dependencies available")
```
