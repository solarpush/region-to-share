# System Dependencies

This application requires certain system-level packages for full functionality.

> **Note**: Since v2.0.0, region-to-share is a **pure Rust application**. There are no Python dependencies.

## Current Application (v2.1.1)

### Core Features

- **Interactive screen region selection** with real-time preview
- **High-performance capture** via native Rust backends
- **Wayland support** via XDG Desktop Portal + PipeWire
- **X11 support** via x11rb + SHM (shared memory)
- **Native egui/eframe UI** — no GTK/Qt dependency
- **Performance monitoring overlay** (FPS, CPU, RAM)
- **Auto-background mode** and region memory

### Runtime Dependencies (system packages)

The binary links against these libraries at runtime — they must be present on the host system.

#### Wayland (Portal/PipeWire)

| Library              | Purpose                                          |
| -------------------- | ------------------------------------------------ |
| `libpipewire-0.3`    | PipeWire client for screen capture               |
| `libspa-0.2`         | SPA plugins used by PipeWire                     |
| `libdbus-1`          | D-Bus for XDG Desktop Portal communication       |
| `xdg-desktop-portal` | Portal backend (provided by the desktop session) |

#### X11

| Library      | Purpose                              |
| ------------ | ------------------------------------ |
| `libx11`     | Core X11                             |
| `libxext`    | X11 extensions (SHM)                 |
| `libxrandr`  | Multi-monitor / resolution detection |
| `libxfixes`  | Cursor compositing                   |
| `libxcursor` | Cursor themes                        |
| `libxi`      | Input events                         |

#### Graphics / Display

| Library              | Purpose                            |
| -------------------- | ---------------------------------- |
| `libgl1` / `libegl1` | OpenGL / EGL for egui rendering    |
| `libgbm1`            | Buffer management (DMA-Buf import) |
| `libwayland-client`  | Wayland protocol (winit/eframe)    |
| `libxkbcommon`       | Keyboard handling                  |

#### Fonts

| Library          | Purpose        |
| ---------------- | -------------- |
| `libfontconfig1` | Font discovery |
| `libfreetype6`   | Font rendering |

---

## Installation by Platform

### Ubuntu / Debian

```bash
sudo apt install -y \
  libpipewire-0.3-0 \
  libspa-0.2-modules \
  libdbus-1-3 \
  xdg-desktop-portal \
  libx11-6 libxext6 libxrandr2 libxfixes3 libxcursor1 libxi6 \
  libegl1 libgl1 libgbm1 \
  libwayland-client0 libxkbcommon0 \
  libfontconfig1 libfreetype6
```

### Fedora / RHEL

```bash
sudo dnf install -y \
  pipewire-libs \
  pipewire-gstreamer \
  dbus-libs \
  xdg-desktop-portal \
  libX11 libXext libXrandr libXfixes libXcursor libXi \
  mesa-libEGL mesa-libGL mesa-libgbm \
  wayland-libs libxkbcommon \
  fontconfig freetype
```

### Arch Linux

```bash
sudo pacman -S --needed \
  pipewire libspa \
  dbus \
  xdg-desktop-portal \
  libx11 libxext libxrandr libxfixes libxcursor libxi \
  mesa libxkbcommon \
  wayland \
  fontconfig freetype2
```

---

## Snap Package

The snap bundles all runtime libraries — no system dependencies needed:

```bash
sudo snap install region-to-share
# or beta channel:
sudo snap install region-to-share --channel=beta
```

**Snap configuration** (`snapcraft.yaml`):

- **Base**: core24 (Ubuntu 24.04 LTS)
- **Confinement**: strict
- **Plugs**: `desktop`, `wayland`, `x11`, `opengl`, `gsettings`, `home`, `unity7`

---

## Flatpak Package

```bash
flatpak install flathub io.github.solarpush.RegionToShare
```

The Flatpak bundles all runtime libraries via the KDE/GNOME runtime.

---

## Build Dependencies

To compile from source (see [BUILD.md](BUILD.md) for full instructions):

```bash
# Ubuntu/Debian
sudo apt install -y \
  cargo rustc pkg-config \
  libclang-dev clang \
  libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxfixes-dev libxi-dev \
  libwayland-dev libxkbcommon-dev \
  libpipewire-0.3-dev libspa-0.2-dev \
  libgl1-mesa-dev libegl1-mesa-dev \
  libdbus-1-dev libfontconfig1-dev libfreetype6-dev libgbm-dev
```

Minimum Rust version: **1.75**

---

## Feature-Specific Requirements

### Wayland Screen Capture

- **Required at runtime**: PipeWire daemon running (pre-installed on most modern distros)
- **Required**: a `xdg-desktop-portal` implementation matching your desktop:
  - GNOME → `xdg-desktop-portal-gnome`
  - KDE → `xdg-desktop-portal-kde`
  - wlroots (Sway, Hyprland) → `xdg-desktop-portal-wlr` or `xdg-desktop-portal-hyprland`

### X11 Screen Capture

- Uses `x11rb` with SHM extension — no extra packages needed beyond the base X11 libs above
- Hardware acceleration available via EGL when Mesa drivers are present

---

## Troubleshooting

**Wayland: "User cancelled" or no portal dialog**
→ Check that `xdg-desktop-portal` and the matching implementation for your DE are installed and running

**PipeWire connection failed**
→ Verify PipeWire is running: `systemctl --user status pipewire`

**X11: Black screen / no capture**
→ Verify the X SHM extension is enabled; check `xdpyinfo | grep -i shm`

**Snap: permission issues**
→ Manually connect interfaces if needed:

```bash
snap connect region-to-share:pipewire
snap connect region-to-share:wayland
```

**High CPU usage**
→ Reduce the framerate in settings (recommended: 30–60 FPS)
