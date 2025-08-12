## 📋 Debug Information

If you encounter a bug (e.g., black screen, crash, unexpected behavior), please provide the following details when opening an issue.  
This information will help us reproduce and fix the problem.

---

### 1. System details
Run the following commands and paste the output:

```bash
cat /etc/os-release
uname -a
```

### 2. Display environment
Identify your display server and desktop environment:

```bash
echo $XDG_SESSION_TYPE
echo $XDG_CURRENT_DESKTOP
gnome-shell --version
```

### 3. Installed library versions
```bash
dpkg -l | grep qt
dpkg -l | grep gtk
```

### 4. Application version
- Exact version of **region-to-share**
- How it was installed (Snap, built from source…)

### 5. GPU and driver information
```bash
glxinfo | grep "OpenGL renderer"
glxinfo | grep "OpenGL version"
```

### 6. Debug logs
Run the application with plugin debug enabled and paste the full output:

```bash
QT_DEBUG_PLUGINS=1 region-to-share
```

If forcing Wayland:
```bash
QT_DEBUG_PLUGINS=1 QT_QPA_PLATFORM=wayland region-to-share
```
