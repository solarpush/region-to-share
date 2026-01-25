# Build Guide - Region to Share

## Dependencies

### Debian/Ubuntu

```bash
sudo apt install -y \
  cargo rustc pkg-config \
  libclang-dev clang \
  libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxfixes-dev libxi-dev libxinerama-dev \
  libwayland-dev libxkbcommon-dev \
  libpipewire-0.3-dev libspa-0.2-dev \
  libgl1-mesa-dev libegl1-mesa-dev \
  libdbus-1-dev libfontconfig1-dev libfreetype6-dev
```

### Fedora

```bash
sudo dnf install -y \
  cargo rust pkg-config \
  clang-devel clang \
  libX11-devel libXext-devel libXrandr-devel libXcursor-devel libXfixes-devel libXi-devel libXinerama-devel \
  wayland-devel libxkbcommon-devel \
  pipewire-devel \
  mesa-libGL-devel mesa-libEGL-devel \
  dbus-devel fontconfig-devel freetype-devel
```

### Arch Linux

```bash
sudo pacman -S --needed \
  rust clang pkgconf \
  libx11 libxext libxrandr libxcursor libxfixes libxi libxinerama \
  wayland libxkbcommon \
  pipewire \
  mesa \
  dbus fontconfig freetype2
```

## Build

### Development build

```bash
cargo build
```

### Release build (optimized)

```bash
cargo build --release
```

### Run

```bash
# Development
./target/debug/region-ui-egui

# Release
./target/release/region-ui-egui

# With debug logging
./target/release/region-ui-egui --debug

# With verbose/trace logging
./target/release/region-ui-egui --verbose
```

## Snap Package

### Build snap

```bash
# Install snapcraft
sudo snap install snapcraft --classic

# Build
snapcraft

# Install locally
sudo snap install --dangerous region-to-share_*.snap
```

## Configuration

Settings are saved in:

```
~/.config/region-to-share/settings.json
```

### Available settings

| Setting                    | Description                       | Default |
| -------------------------- | --------------------------------- | ------- |
| `frame_rate`               | Capture FPS (15-120)              | 60      |
| `capture_mode`             | Capture mode (auto, x11, wayland) | auto    |
| `show_performance`         | Show performance overlay          | false   |
| `window_opacity`           | Window opacity (0.3-1.0)          | 1.0     |
| `auto_send_to_background`  | Auto lower window after selection | false   |
| `remember_last_region`     | Remember last selected region     | false   |
| `auto_use_specific_region` | Auto-start with last region       | false   |
| `global_shortcut`          | Global keyboard shortcut          | ""      |

### Example configuration

```json
{
  "frame_rate": 60,
  "capture_mode": "auto",
  "show_performance": false,
  "window_opacity": 1.0,
  "auto_send_to_background": false,
  "remember_last_region": true,
  "auto_use_specific_region": false,
  "last_region": {
    "x": 100,
    "y": 100,
    "width": 800,
    "height": 600
  },
  "global_shortcut": ""
}
```

## Usage

1. **Launch the application**

2. **Click "Select a region"**
   - The window goes fullscreen with a screenshot overlay
   - Click and drag to select an area
   - Release or press Enter to confirm
   - Press Escape to cancel

3. **Automatic streaming**
   - The window resizes to match the selected region
   - Streaming starts automatically
   - Share this window in Google Meet, Discord, OBS, etc.

4. **Settings**
   - Click the ⚙️ button to access options
   - Adjust frame rate, opacity, etc.
   - Click "Save" to persist changes

## Project Structure

```
region-to-share/
├── Cargo.toml            # Workspace configuration
├── crates/
│   ├── region-core/      # Core types: Rectangle, Frame, PixelFormat
│   ├── region-capture/   # Capture abstraction + X11 backend (XShm)
│   ├── region-portal/    # Wayland backend: Portal + PipeWire + DmaBuf
│   ├── region-config/    # Configuration management
│   └── region-ui-egui/   # GUI application (egui)
├── snapcraft.yaml        # Snap package definition
└── target/               # Build output
```

## Development

### Adding features

1. Edit the relevant crate in `crates/`
2. Rebuild: `cargo build --release`
3. Test: `./target/release/region-ui-egui --debug`

### Running tests

```bash
cargo test --workspace
```

### Code formatting

```bash
cargo fmt --all
```

### Linting

```bash
cargo clippy --workspace
```

## Display Server Support

| Feature            | X11              | Wayland                    |
| ------------------ | ---------------- | -------------------------- |
| Screen capture     | XShm (zero-copy) | Portal + PipeWire + DmaBuf |
| Cursor capture     | XFixes           | Embedded in stream         |
| Window management  | Native           | Portal-based               |
| Send to background | Window lowering  | Minimize                   |
