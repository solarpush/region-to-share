#!/usr/bin/env bash
# Debug collector for region-to-share issues (safe to share publicly)

echo "### 1. System details"
cat /etc/os-release
uname -a | sed 's/'"$(hostname)"'/HOSTNAME/g'
lsb_release -a 2>/dev/null || true

echo
echo "### 2. Display environment"
echo "XDG_SESSION_TYPE=$XDG_SESSION_TYPE"
echo "XDG_CURRENT_DESKTOP=$XDG_CURRENT_DESKTOP"
echo "DISPLAY=$DISPLAY"
xrandr --listmonitors
xrandr --verbose | head -n 20
xdpyinfo | grep dimensions

echo
echo "### 3. Installed libraries"
dpkg -l | grep -E "qt|gtk|pipewire|xdg-desktop-portal" || true

echo
echo "### 4. GPU and driver"
glxinfo | grep "OpenGL renderer"
glxinfo | grep "OpenGL version"
lspci -k | grep -A 2 -E "VGA|3D"

echo
echo "### 5. PipeWire / Portal"
systemctl --user status pipewire --no-pager
systemctl --user status wireplumber --no-pager 2>/dev/null || systemctl --user status pipewire-media-session --no-pager
ps -ef | grep xdg-desktop-portal
