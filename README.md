# Region to Share 📺

[![region-to-share](https://snapcraft.io/region-to-share/badge.svg)](https://snapcraft.io/region-to-share)

A simple and efficient Linux application for sharing specific areas of your screen in video calls (Google Meet, Teams, Discord, etc.).

## 🚀 Features

- ✅ **Interactive area selection** with click and drag
- ✅ **Real-time display window** (30 FPS) of the selected area
- ✅ **Direct sharing** in video conferencing apps via "Share window"
- ✅ **Intuitive controls**: Pause/Resume, Refresh
- ✅ **Cursor visibility** in captured area
- ✅ **Exact window ratio** without white borders
- ✅ **Compatible** with all Linux distributions (X11/Wayland)
- ✅ **Lightweight and fast**: No complex dependencies

## 🎯 How it works

1. **Launch the application** → Select a screen area
2. **A window opens** → Shows real-time content of this area
3. **In your video app** → "Share screen" → "Window" → Select "Region to Share"
4. **✅ You share only this area!**

## 📋 Requirements

- Linux (any modern distribution)
- Python 3.8+
- PyQt5 (installed automatically)

## Usage

```bash
region-to-share --help #to display all supported commands

#output
usage: regiontoshare => main.py [-h] [--mode {auto,portal-screencast,mss}] [--debug] [--version]

Region-to-Share: Select and share screen regions for video conferencing

options:
  -h, --help            show this help message and exit
  --mode {auto,portal-screencast,mss}, --capture-mode {auto,portal-screencast,mss}
                        Force a specific capture method (default: auto-detect)
  --debug               Enable debug output
  --version             show program's version number and exit

Examples:
  region-to-share                    # Auto-detect best method
  region-to-share --mode portal-screencast  # Force Portal ScreenCast
  region-to-share --mode mss         # Force MSS capture
```

## 🔧 Installation

### Via Snap Store (recommended)

```bash
# Install from Snap Store
sudo snap install region-to-share
```

### From source code

````bash
# Clone the repository
git clone https://github.com/solarpush/region-to-share.git
cd region-to-share

# Setup virtual environment
./run_venv.sh
```

## 🎯 Usage

### Launch

```bash
./run.sh
```

### Simple steps

1. **Launch the application**: `./run.sh`
2. **Select a region**: Click and drag on your screen
3. **Display window**: A window opens with your region in real-time
4. **Share in video conference**:
   - Google Meet/Teams/Discord: "Share screen" → "Window"
   - Select "Region to Share - Selected Region"
   - ✅ You share only this region!

### Controls

- **⏸️ Pause/▶️ Resume**: Stop/resume capture
- **🔄 Refresh**: Force update
- **❌ Close**: Close application

## 🛠️ Architecture

```
region_to_share/
├── main.py              # Main entry point
├── screen_selector.py   # Interactive region selection
├── display_window.py    # Real-time display window
└── __init__.py          # Python package
```

### Technologies

- **PyQt5**: Modern graphical interface
- **mss**: High-performance screen capture (x11)
- **xdg_portal**: wayland portal api (wayland)
- **OpenCV + NumPy**: Efficient image processing
- **Snapcraft**: Universal Linux packaging

## 📦 Snap Package

### Building the snap (for try packing before send PR)

```bash
# Install snapcraft
sudo snap install snapcraft --classic

# Build the snap
snapcraft #optional use --use-lxd

# Install
sudo snap install --devmode *.snap
```

## 🤝 Contributing

1. Fork the project
2. Create a feature branch
3. Commit your changes
4. Create a Pull Request

You can extend capture support for different Linux desktop environments by editing `./region_to_share/universal_capture.py`.
Currently supported XDG_SESSION_TYPE values:
- `wayland`: Uses the `xdg-desktop-portal` API for screen capture. Ensure that the appropriate backend (e.g., `xdg-desktop-portal-kde` for KDE or `xdg-desktop-portal-gnome` for GNOME) is installed and running. Compatibility may vary depending on the compositor (e.g., KWin for KDE, Mutter for GNOME).
- `x11`: Relies on the `mss` library for screen capture. This works well with most X11-based environments but may encounter issues with minimal window managers or restricted X11 configurations.

Other session types (e.g. `mir`, `tty`) are not supported yet, but contributions are welcome! For example, adding support for `mir` would require implementing a Mir-specific API, and `tty` would need a different approach entirely.
## 📄 License

MIT License - see the [LICENSE](LICENSE) file for details.

---

**Region to Share** - Simplified screen region sharing for Linux 🐧
````

[![region-to-share](https://snapcraft.io/region-to-share/badge.svg)](https://snapcraft.io/region-to-share)
