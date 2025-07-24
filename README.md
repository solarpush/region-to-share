# Region to Share 📺

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

## 🔧 Installation

### Via Snap Store (recommended)

```bash
# Install from Snap Store
sudo snap install region-to-share
```

### Via local snap file

```bash
# Install from local snap file
sudo snap install --dangerous region-to-share_1.0.0_amd64.snap
```

### From source code

````bash
# Clone the repository
git clone https://github.com/solarpush/region-to-share.git
cd region-to-share

# Setup virtual environment
./run_venv.sh

# Launch the application
source venv_region/bin/activate
python -m region_to_share.main
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
- **mss**: High-performance screen capture
- **OpenCV + NumPy**: Efficient image processing
- **Snapcraft**: Universal Linux packaging

## 📦 Snap Package

### Building the snap

```bash
# Install snapcraft
sudo snap install snapcraft --classic

# Build the snap
snapcraft

# Install
sudo snap install --devmode *.snap
```

## 🤝 Contributing

1. Fork the project
2. Create a feature branch
3. Commit your changes
4. Create a Pull Request

## 📄 License

MIT License - see the [LICENSE](LICENSE) file for details.

---

**Region to Share** - Simplified screen region sharing for Linux 🐧
````
