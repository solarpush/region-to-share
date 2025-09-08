## 📋 Debug Information

If you encounter a bug (e.g., black screen, crash, unexpected behavior), please provide the following details when opening an issue.  
This information will help us reproduce and fix the problem.

You can run directly a debug script from :

```bash
# wget
bash <(wget -qO- https://raw.githubusercontent.com/solarpush/region-to-share/main/debug-script.sh)
#or
#curl
bash <(curl -s  https://raw.githubusercontent.com/solarpush/region-to-share/main/debug-script.sh)

```

---

### 1. System details

```bash
cat /etc/os-release
uname -a
lsb_release -a 2>/dev/null || true
```

### 2. Display environment

Identify your display server and desktop environment:

```bash
echo $XDG_SESSION_TYPE
echo $XDG_CURRENT_DESKTOP
echo $DISPLAY
xrandr --listmonitors
xrandr --verbose | head -n 20
xdpyinfo | grep dimensions
```

### 3. Installed library versions

```bash
dpkg -l | grep -E "qt|gtk|pipewire|xdg-desktop-portal"
```

### 4. Application version

- Exact version of **region-to-share**
- How it was installed (Snap, built from source…)

### 5. GPU and driver information

```bash
glxinfo | grep "OpenGL renderer"
glxinfo | grep "OpenGL version"
lspci -k | grep -A 2 -E "VGA|3D"
```

### 6. Debug logs

Run the application with plugin debug enabled and paste the full output:

```bash
region-to-share --debug
```

If forcing Wayland:

```bash
region-to-share --debug --mode portal-screencast
```
