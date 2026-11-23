//! Cross-platform OS abstractions for Region to Share v3.
//!
//! Isolates every platform-specific detail (display server & DE detection,
//! resource metrics, system color-scheme detection, window Z-order control,
//! clipboard image/text copy, and standard directories) so that Linux
//! (Wayland/X11), macOS, and Windows share the same upper-layer architecture.

use region_core::PixelFormat;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Active display server protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayServer {
    Wayland,
    X11,
    MacOsQuartz,
    WindowsDwm,
    Unknown,
}

impl DisplayServer {
    pub fn detect() -> Self {
        #[cfg(target_os = "linux")]
        {
            if std::env::var("WAYLAND_DISPLAY").is_ok()
                || std::env::var("XDG_SESSION_TYPE")
                    .map(|t| t.eq_ignore_ascii_case("wayland"))
                    .unwrap_or(false)
            {
                return Self::Wayland;
            }
            if std::env::var("DISPLAY").is_ok()
                || std::env::var("XDG_SESSION_TYPE")
                    .map(|t| t.eq_ignore_ascii_case("x11"))
                    .unwrap_or(false)
            {
                return Self::X11;
            }
            Self::Unknown
        }
        #[cfg(target_os = "macos")]
        {
            Self::MacOsQuartz
        }
        #[cfg(target_os = "windows")]
        {
            Self::WindowsDwm
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            Self::Unknown
        }
    }

    pub const fn is_wayland(self) -> bool {
        matches!(self, Self::Wayland)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Wayland => "Wayland",
            Self::X11 => "X11",
            Self::MacOsQuartz => "macOS Quartz",
            Self::WindowsDwm => "Windows DWM",
            Self::Unknown => "Unknown",
        }
    }
}

/// Active desktop environment (used for portal strategy & native integrations).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopEnvironment {
    KdePlasma,
    Gnome,
    Hyprland,
    Sway,
    Cosmic,
    OtherLinux,
    MacOs,
    Windows,
}

impl DesktopEnvironment {
    pub fn detect() -> Self {
        #[cfg(target_os = "linux")]
        {
            let raw = std::env::var("XDG_CURRENT_DESKTOP")
                .or_else(|_| std::env::var("DESKTOP_SESSION"))
                .unwrap_or_default()
                .to_lowercase();

            if raw.contains("kde") || raw.contains("plasma") {
                Self::KdePlasma
            } else if raw.contains("gnome") || raw.contains("ubuntu") {
                Self::Gnome
            } else if raw.contains("hyprland") {
                Self::Hyprland
            } else if raw.contains("sway") {
                Self::Sway
            } else if raw.contains("cosmic") {
                Self::Cosmic
            } else {
                Self::OtherLinux
            }
        }
        #[cfg(target_os = "macos")]
        {
            Self::MacOs
        }
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            Self::OtherLinux
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::KdePlasma => "KDE Plasma",
            Self::Gnome => "GNOME",
            Self::Hyprland => "Hyprland",
            Self::Sway => "Sway",
            Self::Cosmic => "COSMIC",
            Self::OtherLinux => "Linux",
            Self::MacOs => "macOS",
            Self::Windows => "Windows",
        }
    }

    /// True if the compositor supports `zwlr_layer_shell_v1` overlays.
    pub const fn supports_wlr_layer_shell(self) -> bool {
        matches!(
            self,
            Self::KdePlasma | Self::Hyprland | Self::Sway | Self::Cosmic
        )
    }

    /// True if the desktop portal provides a native "Rectangular Region" picker
    /// inside `org.freedesktop.portal.ScreenCast` (KDE Plasma 6 / KWin `stream_region`).
    pub const fn has_native_portal_region_picker(self) -> bool {
        matches!(self, Self::KdePlasma)
    }
}

/// Packaging environment detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageFormat {
    Flatpak,
    Snap,
    Native,
}

impl PackageFormat {
    pub fn detect() -> Self {
        if std::env::var("FLATPAK_ID").is_ok() {
            Self::Flatpak
        } else if std::env::var("SNAP").is_ok() {
            Self::Snap
        } else {
            Self::Native
        }
    }

    pub fn launch_command(self) -> &'static str {
        match self {
            Self::Flatpak => "flatpak run io.github.solarpush.RegionToShare",
            Self::Snap | Self::Native => "region-to-share",
        }
    }
}

/// Standard directories used by the application.
pub struct AppPaths;

impl AppPaths {
    pub fn videos_dir() -> PathBuf {
        dirs::video_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Videos")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("RegionToShare")
    }

    pub fn pictures_dir() -> PathBuf {
        dirs::picture_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("RegionToShare")
    }

    pub fn ipc_socket_path() -> PathBuf {
        let dir = std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        dir.join("region-to-share.sock")
    }
}

/// Detected OS light/dark preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemColorScheme {
    Dark,
    Light,
}

/// Query the OS preferred color scheme (async XDG Settings portal on Linux).
pub async fn detect_system_color_scheme() -> SystemColorScheme {
    #[cfg(target_os = "linux")]
    {
        if let Ok(settings) = ashpd::desktop::settings::Settings::new().await {
            if let Ok(scheme) = settings.color_scheme().await {
                return match scheme {
                    ashpd::desktop::settings::ColorScheme::PreferLight => SystemColorScheme::Light,
                    _ => SystemColorScheme::Dark,
                };
            }
        }
    }
    SystemColorScheme::Dark
}

/// Cross-platform process CPU & RAM usage monitor.
pub struct ResourceMonitor {
    last_cpu_time: u64,
    last_check: Instant,
    num_cpus: usize,
}

impl ResourceMonitor {
    pub fn new() -> Self {
        Self {
            last_cpu_time: Self::get_process_cpu_time(),
            last_check: Instant::now(),
            num_cpus: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
        }
    }

    /// Returns `(cpu_percent_total, memory_rss_mb)`.
    pub fn sample(&mut self) -> (f64, f64) {
        (self.cpu_percent(), Self::memory_mb())
    }

    fn cpu_percent(&mut self) -> f64 {
        let current = Self::get_process_cpu_time();
        let elapsed = self.last_check.elapsed();
        if elapsed.as_millis() < 100 {
            return 0.0;
        }
        let delta = current.saturating_sub(self.last_cpu_time);
        let elapsed_ticks = (elapsed.as_secs_f64() * 100.0) as u64;
        let per_core = if elapsed_ticks > 0 {
            (delta as f64 / elapsed_ticks as f64) * 100.0
        } else {
            0.0
        };
        self.last_cpu_time = current;
        self.last_check = Instant::now();
        (per_core / self.num_cpus as f64).min(100.0)
    }

    #[cfg(target_os = "linux")]
    fn get_process_cpu_time() -> u64 {
        if let Ok(stat) = std::fs::read_to_string("/proc/self/stat") {
            let parts: Vec<&str> = stat.split_whitespace().collect();
            if parts.len() > 14 {
                let utime: u64 = parts[13].parse().unwrap_or(0);
                let stime: u64 = parts[14].parse().unwrap_or(0);
                return utime + stime;
            }
        }
        0
    }

    #[cfg(not(target_os = "linux"))]
    fn get_process_cpu_time() -> u64 {
        0
    }

    #[cfg(target_os = "linux")]
    pub fn memory_mb() -> f64 {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    if let Some(kb) = rest
                        .split_whitespace()
                        .next()
                        .and_then(|s| s.parse::<f64>().ok())
                    {
                        return kb / 1024.0;
                    }
                }
            }
        }
        0.0
    }

    #[cfg(not(target_os = "linux"))]
    pub fn memory_mb() -> f64 {
        0.0
    }
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect an active, writable `v4l2loopback` virtual camera device (e.g. `/dev/video2`).
///
/// When `v4l2loopback` is loaded, Chrome, Google Meet, Discord, Slack, and Zoom
/// immediately discover it as a standard hardware webcam with ZERO browser flags.
pub fn detect_v4l2loopback_device() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let v4l_dir = Path::new("/sys/devices/virtual/video4linux");
        let entries = std::fs::read_dir(v4l_dir).ok()?;
        let mut candidates: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let dev_path = PathBuf::from("/dev").join(&name);
            if dev_path.exists()
                && std::fs::OpenOptions::new()
                    .write(true)
                    .open(&dev_path)
                    .is_ok()
            {
                candidates.push(dev_path);
            }
        }
        candidates.sort();
        candidates.into_iter().next()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Run a transient one-shot KWin 6 (or KWin 5 fallback) JavaScript snippet over session D-Bus
/// (`org.kde.KWin /Scripting`) without requiring `qdbus` CLI binaries.
///
/// Uses direct `Connection::call_method` instead of `Proxy::new` so `zbus` never triggers
/// upfront `Properties.GetAll` / `AddMatch` calls on KWin scripting objects that expose no properties.
#[cfg(target_os = "linux")]
async fn run_kwin_script(js_source: &str) -> bool {
    use zbus::Connection;

    let conn = match Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[KWin] Failed to connect to session D-Bus: {e}");
            return false;
        }
    };
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let script_path = runtime_dir.join(format!("rts-kwin-{}-{nonce}.js", std::process::id()));
    if let Err(e) = std::fs::write(&script_path, js_source) {
        log::warn!(
            "[KWin] Failed to write temporary script {}: {e}",
            script_path.display()
        );
        return false;
    }

    let path_str = script_path.to_string_lossy().into_owned();
    let result = async {
        let reply = match conn
            .call_method(
                Some("org.kde.KWin"),
                "/Scripting",
                Some("org.kde.kwin.Scripting"),
                "loadScript",
                &(path_str.as_str(),),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                log::warn!("[KWin] loadScript D-Bus call failed: {e}");
                return false;
            }
        };
        let script_id: i32 = match reply.body().deserialize() {
            Ok(id) => id,
            Err(e) => {
                log::warn!("[KWin] loadScript reply deserialize failed: {e}");
                return false;
            }
        };
        if script_id < 0 {
            log::warn!("[KWin] loadScript returned invalid script id: {script_id}");
            return false;
        }

        // Plasma 6 uses `/Scripting/Script<id>`; Plasma 5 used `/<id>`
        let plasma6_path = format!("/Scripting/Script{script_id}");
        let plasma5_path = format!("/{script_id}");

        match conn
            .call_method(
                Some("org.kde.KWin"),
                plasma6_path.as_str(),
                Some("org.kde.kwin.Script"),
                "run",
                &(),
            )
            .await
        {
            Ok(_) => {
                let _ = conn
                    .call_method(
                        Some("org.kde.KWin"),
                        plasma6_path.as_str(),
                        Some("org.kde.kwin.Script"),
                        "stop",
                        &(),
                    )
                    .await;
                true
            }
            Err(e6) => {
                log::debug!(
                    "[KWin] Plasma 6 path {plasma6_path} failed ({e6}), trying Plasma 5 fallback"
                );
                match conn
                    .call_method(
                        Some("org.kde.KWin"),
                        plasma5_path.as_str(),
                        Some("org.kde.kwin.Script"),
                        "run",
                        &(),
                    )
                    .await
                {
                    Ok(_) => {
                        let _ = conn
                            .call_method(
                                Some("org.kde.KWin"),
                                plasma5_path.as_str(),
                                Some("org.kde.kwin.Script"),
                                "stop",
                                &(),
                            )
                            .await;
                        true
                    }
                    Err(e5) => {
                        log::warn!(
                            "[KWin] Script run failed on Plasma 6 ({e6}) and Plasma 5 ({e5})"
                        );
                        false
                    }
                }
            }
        }
    }
    .await;

    let _ = std::fs::remove_file(&script_path);
    result
}

#[cfg(target_os = "linux")]
fn js_string_literal(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "");
    format!("\"{escaped}\"")
}

#[cfg(target_os = "linux")]
fn kwin_target_matcher_js(window_title: &str) -> String {
    let title_json = js_string_literal(window_title);
    let pid = std::process::id();
    format!(
        r#"
var target = {title_json};
var myPid = {pid};
function isTargetWindow(w) {{
    if (!w) return false;
    var cap = (w.caption || "");
    var capN = (w.captionNormal || "");
    if (cap === "Region to Share Controls" || capN === "Region to Share Controls" ||
        cap.indexOf("Region to Share Controls") !== -1 || capN.indexOf("Region to Share Controls") !== -1) return false;
    if (cap === target || capN === target) return true;
    if (myPid > 0 && w.pid === myPid && !w.popupWindow && !w.tooltip && !w.notification) return true;
    return false;
}}
function currentDesktopFor(w) {{
    var cur = null;
    if (typeof workspace.currentDesktopForScreen === "function" && w && w.output) {{
        try {{ cur = workspace.currentDesktopForScreen(w.output); }} catch (e) {{}}
    }}
    if (!cur) cur = workspace.currentDesktop;
    return cur;
}}
"#
    )
}

/// Bring the mirror / app window back to the active virtual desktop, clear `keepBelow` /
/// `skipSwitcher`, raise it, and focus it. Works on Wayland (KDE Plasma 6) and X11.
pub async fn bring_window_to_front(window_title: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        if !DisplayServer::detect().is_wayland() {
            return raise_x11_window(window_title);
        }
        if DesktopEnvironment::detect() != DesktopEnvironment::KdePlasma {
            return false;
        }
        let prelude = kwin_target_matcher_js(window_title);
        let js = format!(
            r#"
{prelude}
var wins = (typeof workspace.windowList === "function") ? workspace.windowList() : workspace.clientList();
for (var i = 0; i < wins.length; i++) {{
    var w = wins[i];
    if (isTargetWindow(w)) {{
        w.minimized = false;
        w.keepBelow = false;
        w.skipSwitcher = false;
        w.onAllDesktops = false;
        var cur = currentDesktopFor(w);
        if (cur) {{
            w.desktops = [cur];
        }}
        if (typeof workspace.raiseWindow === "function") {{
            workspace.raiseWindow(w);
        }}
        workspace.activeWindow = w;
        break;
    }}
}}
"#
        );
        run_kwin_script(&js).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = window_title;
        false
    }
}

/// Minimize or restore the mirror window via compositor control (Wayland/KDE Plasma).
///
/// Wayland compositors keep streaming minimized windows via PipeWire screencast, whereas
/// X11/XComposite requires keeping the window mapped (use `set_window_keep_below` on X11).
pub async fn set_window_minimized(window_title: &str, minimized: bool) -> bool {
    #[cfg(target_os = "linux")]
    {
        if !minimized {
            return bring_window_to_front(window_title).await;
        }
        if !DisplayServer::detect().is_wayland() {
            return false;
        }
        if DesktopEnvironment::detect() != DesktopEnvironment::KdePlasma {
            return false;
        }
        let prelude = kwin_target_matcher_js(window_title);
        let js = format!(
            r#"
{prelude}
var wins = (typeof workspace.windowList === "function") ? workspace.windowList() : workspace.clientList();
for (var i = 0; i < wins.length; i++) {{
    var w = wins[i];
    if (isTargetWindow(w)) {{
        w.keepAbove = false;
        w.minimized = true;
        break;
    }}
}}
"#
        );
        run_kwin_script(&js).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window_title, minimized);
        false
    }
}

/// Open a native file chooser dialog to pick a pause video file.
/// Runs synchronously on a blocking helper thread when invoked from async code.
pub fn pick_video_file_blocking(start_dir: Option<&Path>) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let dir_str = start_dir
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().into_owned())
            .or_else(|| {
                dirs::video_dir()
                    .or_else(dirs::home_dir)
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| ".".to_string());

        let try_kdialog = || -> Option<PathBuf> {
            let out = Command::new("kdialog")
                .args([
                    "--getopenfilename",
                    &dir_str,
                    "Video Files (*.mp4 *.mkv *.webm *.mov *.gif *.avi *.m4v);;All Files (*)",
                    "--title",
                    "Region to Share",
                ])
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if path.is_empty() {
                None
            } else {
                Some(PathBuf::from(path))
            }
        };

        let try_zenity = || -> Option<PathBuf> {
            let out = Command::new("zenity")
                .args([
                    "--file-selection",
                    "--title=Region to Share",
                    &format!("--filename={}/", dir_str.trim_end_matches('/')),
                    "--file-filter=Video files | *.mp4 *.mkv *.webm *.mov *.gif *.avi *.m4v",
                    "--file-filter=All files | *",
                ])
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if path.is_empty() {
                None
            } else {
                Some(PathBuf::from(path))
            }
        };

        if DesktopEnvironment::detect() == DesktopEnvironment::KdePlasma {
            try_kdialog().or_else(try_zenity)
        } else {
            try_zenity().or_else(try_kdialog)
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = start_dir;
        None
    }
}

/// Keep the mirror window **behind all normal windows** (`keepBelow = true`, NOT minimized!)
/// so it stays 100% mapped, rendered at exact 1:1 pixels, and selectable in Chrome/Meet/Discord
/// on Wayland (KDE Plasma 6) and X11!
pub async fn set_window_keep_below(window_title: &str, keep_below: bool) -> bool {
    #[cfg(target_os = "linux")]
    {
        if !keep_below {
            return bring_window_to_front(window_title).await;
        }
        if !DisplayServer::detect().is_wayland() {
            lower_active_window();
            return true;
        }
        if DesktopEnvironment::detect() != DesktopEnvironment::KdePlasma {
            return false;
        }
        let prelude = kwin_target_matcher_js(window_title);
        let js = format!(
            r#"
{prelude}
var wins = (typeof workspace.windowList === "function") ? workspace.windowList() : workspace.clientList();
for (var i = 0; i < wins.length; i++) {{
    var w = wins[i];
    if (isTargetWindow(w)) {{
        w.minimized = false;
        w.keepAbove = false;
        w.keepBelow = true;
        w.skipSwitcher = true;
        break;
    }}
}}
"#
        );
        run_kwin_script(&js).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window_title, keep_below);
        false
    }
}

/// Ensure at least 2 KWin virtual desktops exist via `org.kde.KWin /VirtualDesktopManager`.
/// Uses direct `Connection::call_method` (no `Proxy` property caching / `GetAll`).
#[cfg(target_os = "linux")]
async fn ensure_kwin_second_desktop() {
    use zbus::zvariant::OwnedValue;
    use zbus::Connection;

    let Ok(conn) = Connection::session().await else {
        return;
    };
    let count: u32 = match conn
        .call_method(
            Some("org.kde.KWin"),
            "/VirtualDesktopManager",
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.kde.KWin.VirtualDesktopManager", "count"),
        )
        .await
    {
        Ok(reply) => reply
            .body()
            .deserialize::<OwnedValue>()
            .ok()
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(1),
        Err(e) => {
            log::debug!("[KWin] VirtualDesktopManager Get(count) failed: {e}");
            1
        }
    };
    if count < 2 {
        if let Err(e) = conn
            .call_method(
                Some("org.kde.KWin"),
                "/VirtualDesktopManager",
                Some("org.kde.KWin.VirtualDesktopManager"),
                "createDesktop",
                &(count, "Region to Share"),
            )
            .await
        {
            log::warn!("[KWin] VirtualDesktopManager createDesktop failed: {e}");
        }
    }
}

/// Move the mirror window to a secondary/hidden virtual desktop (without switching away from the
/// user's current desktop) — keeps the window off your workspace while staying mapped!
/// Pass `to_hidden = false` to bring it right back to the active desktop.
pub async fn set_window_hidden_desktop(window_title: &str, to_hidden: bool) -> bool {
    #[cfg(target_os = "linux")]
    {
        if !to_hidden {
            return bring_window_to_front(window_title).await;
        }
        if !DisplayServer::detect().is_wayland() {
            return move_active_window_to_other_desktop();
        }
        if DesktopEnvironment::detect() != DesktopEnvironment::KdePlasma {
            return false;
        }
        // Best-effort D-Bus pre-creation (Plasma 5/6), plus synchronous `workspace.createDesktop`
        // inside the KWin 6 script itself so there is zero race condition with `workspace.desktops`.
        ensure_kwin_second_desktop().await;

        let prelude = kwin_target_matcher_js(window_title);
        let js = format!(
            r#"
{prelude}
var wins = (typeof workspace.windowList === "function") ? workspace.windowList() : workspace.clientList();
for (var i = 0; i < wins.length; i++) {{
    var w = wins[i];
    if (isTargetWindow(w)) {{
        w.minimized = false;
        w.keepAbove = false;
        w.keepBelow = true;
        w.skipSwitcher = true;
        var desks = workspace.desktops;
        if ((!desks || desks.length < 2) && typeof workspace.createDesktop === "function") {{
            workspace.createDesktop(desks ? desks.length : 1, "Region to Share");
            desks = workspace.desktops;
        }}
        if (desks && desks.length >= 2) {{
            var cur = currentDesktopFor(w);
            var curId = cur ? cur.id : null;
            var dest = null;
            for (var j = 0; j < desks.length; j++) {{
                if (desks[j] && desks[j] !== cur && (!curId || desks[j].id !== curId)) {{
                    dest = desks[j];
                    break;
                }}
            }}
            if (dest !== null) {{
                w.onAllDesktops = false;
                w.desktops = [dest];
            }}
        }}
        break;
    }}
}}
"#
        );
        run_kwin_script(&js).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window_title, to_hidden);
        false
    }
}

/// Bring our X11 window back to the current virtual desktop, raise it above normal windows,
/// and request activation via EWMH (`_NET_WM_DESKTOP` + `_NET_ACTIVE_WINDOW`).
#[cfg(target_os = "linux")]
fn raise_x11_window(_window_title: &str) -> bool {
    if DisplayServer::detect().is_wayland() {
        return false;
    }
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        ClientMessageData, ClientMessageEvent, ConfigureWindowAux, ConnectionExt, EventMask,
        StackMode,
    };

    let Ok((conn, screen_num)) = x11rb::connect(None) else {
        return false;
    };
    let screen = &conn.setup().roots[screen_num];
    let intern = |name: &[u8]| {
        conn.intern_atom(false, name)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.atom)
    };
    let (Some(a_clients), Some(a_pid), Some(a_cur), Some(a_desk), Some(a_active)) = (
        intern(b"_NET_CLIENT_LIST"),
        intern(b"_NET_WM_PID"),
        intern(b"_NET_CURRENT_DESKTOP"),
        intern(b"_NET_WM_DESKTOP"),
        intern(b"_NET_ACTIVE_WINDOW"),
    ) else {
        return false;
    };

    let my_pid = std::process::id();
    let cur_desk = conn
        .get_property(false, screen.root, a_cur, x11rb::NONE, 0, 1)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|p| p.value32()?.next());

    let clients: Vec<u32> = conn
        .get_property(false, screen.root, a_clients, x11rb::NONE, 0, 1024)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|p| p.value32().map(|it| it.collect()))
        .unwrap_or_default();

    let mut target_win = None;
    for win_id in clients {
        let win_pid = conn
            .get_property(false, win_id, a_pid, x11rb::NONE, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|p| p.value32()?.next());
        if win_pid == Some(my_pid) {
            target_win = Some(win_id);
            break;
        }
    }

    let Some(win_id) = target_win else {
        return false;
    };

    if let Some(desk) = cur_desk {
        let move_ev = ClientMessageEvent {
            response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window: win_id,
            type_: a_desk,
            data: ClientMessageData::from([desk, 2, 0, 0, 0]),
        };
        let _ = conn.send_event(
            false,
            screen.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            move_ev,
        );
    }

    let aux = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
    let _ = conn.configure_window(win_id, &aux);

    let act_ev = ClientMessageEvent {
        response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
        format: 32,
        sequence: 0,
        window: win_id,
        type_: a_active,
        data: ClientMessageData::from([2, 0, 0, 0, 0]),
    };
    let ok = conn
        .send_event(
            false,
            screen.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            act_ev,
        )
        .is_ok();
    let _ = conn.flush();
    ok
}

/// Move the active window to another X11 virtual desktop via EWMH `_NET_WM_DESKTOP`.
/// Keeps the XComposite backing pixmap mapped and rendering at full 1:1 resolution
/// while keeping the window completely off the user's current workspace!
pub fn move_active_window_to_other_desktop() -> bool {
    #[cfg(target_os = "linux")]
    {
        if DisplayServer::detect().is_wayland() {
            return false;
        }
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::{
            ClientMessageData, ClientMessageEvent, ConnectionExt, EventMask,
        };

        let Ok((conn, screen_num)) = x11rb::connect(None) else {
            return false;
        };
        let screen = &conn.setup().roots[screen_num];
        let intern = |name: &[u8]| {
            conn.intern_atom(false, name)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| r.atom)
        };
        let (Some(a_active), Some(a_num), Some(a_cur), Some(a_desk)) = (
            intern(b"_NET_ACTIVE_WINDOW"),
            intern(b"_NET_NUMBER_OF_DESKTOPS"),
            intern(b"_NET_CURRENT_DESKTOP"),
            intern(b"_NET_WM_DESKTOP"),
        ) else {
            return false;
        };

        let read_u32 = |atom| {
            conn.get_property(false, screen.root, atom, x11rb::NONE, 0, 1)
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|p| p.value32()?.next())
        };

        let (Some(win_id), Some(num_desks), Some(cur_desk)) =
            (read_u32(a_active), read_u32(a_num), read_u32(a_cur))
        else {
            return false;
        };
        if num_desks < 2 {
            return false;
        }
        let target_desk = (cur_desk + 1) % num_desks;
        let event = ClientMessageEvent {
            response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window: win_id,
            type_: a_desk,
            data: ClientMessageData::from([target_desk, 2, 0, 0, 0]),
        };
        let ok = conn
            .send_event(
                false,
                screen.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .is_ok();
        let _ = conn.flush();
        return ok;
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Lower the active window to the bottom of the Z-stack on X11.
/// No-op on Wayland (compositor controls Z-order; caller uses minimize).
pub fn lower_active_window() {
    #[cfg(target_os = "linux")]
    {
        if DisplayServer::detect().is_wayland() {
            return;
        }
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt, StackMode};

        let Ok((conn, screen_num)) = x11rb::connect(None) else {
            return;
        };
        let screen = &conn.setup().roots[screen_num];
        let active_atom = conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.atom);

        if let Some(atom) = active_atom {
            if let Ok(reply) = conn.get_property(false, screen.root, atom, x11rb::NONE, 0, 1) {
                if let Ok(prop) = reply.reply() {
                    if let Some(win_id) = prop.value32().and_then(|mut i| i.next()) {
                        let aux = ConfigureWindowAux::new().stack_mode(StackMode::BELOW);
                        let _ = conn.configure_window(win_id, &aux);
                        let _ = conn.flush();
                    }
                }
            }
        }
    }
}

/// Encode raw BGRA/RGBA frame pixels into an in-memory PNG byte vector.
pub fn encode_frame_png(
    width: u32,
    height: u32,
    format: PixelFormat,
    pixels: &[u8],
) -> Result<Vec<u8>, String> {
    let rgba = to_rgba_bytes(width, height, format, pixels)?;
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG header failed: {e}"))?;
        writer
            .write_image_data(&rgba)
            .map_err(|e| format!("PNG encode failed: {e}"))?;
    }
    Ok(out)
}

/// Save raw BGRA/RGBA frame pixels as a PNG file.
pub fn save_frame_png(
    path: &Path,
    width: u32,
    height: u32,
    format: PixelFormat,
    pixels: &[u8],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {e}"))?;
    }
    let png_bytes = encode_frame_png(width, height, format, pixels)?;
    std::fs::write(path, png_bytes).map_err(|e| format!("Failed to write PNG: {e}"))
}

/// Copy raw BGRA/RGBA frame pixels as PNG to the OS clipboard.
/// Uses native `wl-copy` on Wayland and `xclip`/`xsel` on X11 to avoid X11 clipboard timeouts.
pub fn copy_frame_to_clipboard(
    width: u32,
    height: u32,
    format: PixelFormat,
    pixels: &[u8],
) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let png_bytes = encode_frame_png(width, height, format, pixels)?;

    #[cfg(target_os = "linux")]
    {
        let candidates: &[(&str, &[&str])] = if DisplayServer::detect().is_wayland() {
            &[
                ("wl-copy", &["--type", "image/png"]),
                ("xclip", &["-selection", "clipboard", "-t", "image/png"]),
            ]
        } else {
            &[
                ("xclip", &["-selection", "clipboard", "-t", "image/png"]),
                ("wl-copy", &["--type", "image/png"]),
            ]
        };

        for (bin, args) in candidates {
            if let Ok(mut child) = Command::new(bin)
                .args(*args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    if stdin.write_all(&png_bytes).is_ok() {
                        drop(stdin);
                        if child.wait().map(|s| s.success()).unwrap_or(false) {
                            return Ok(());
                        }
                    }
                }
            }
        }
        Err("Install `wl-clipboard` (Wayland) or `xclip` (X11) for PNG clipboard support".into())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = png_bytes;
        Err("PNG clipboard integration for macOS/Windows is planned for Phase 6".into())
    }
}

/// Copy UTF-8 text to the OS clipboard.
pub fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {e}"))?;
    clipboard
        .set_text(text.to_owned())
        .map_err(|e| format!("Failed to set clipboard text: {e}"))
}

/// Convert raw frame bytes to tightly packed RGBA8888 (alpha forced opaque).
pub fn to_rgba_bytes(
    width: u32,
    height: u32,
    format: PixelFormat,
    pixels: &[u8],
) -> Result<Vec<u8>, String> {
    let count = (width as usize).saturating_mul(height as usize);
    let bpp = format.bytes_per_pixel();
    if pixels.len() < count * bpp {
        return Err(format!(
            "Buffer too small: {} < {} bytes",
            pixels.len(),
            count * bpp
        ));
    }
    let mut out = Vec::with_capacity(count * 4);
    match format {
        PixelFormat::RGBA8888 => {
            for px in pixels.chunks_exact(4).take(count) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        PixelFormat::BGRA8888 => {
            for px in pixels.chunks_exact(4).take(count) {
                out.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
        PixelFormat::ARGB8888 => {
            for px in pixels.chunks_exact(4).take(count) {
                out.extend_from_slice(&[px[1], px[2], px[3], 255]);
            }
        }
        PixelFormat::RGB888 => {
            for px in pixels.chunks_exact(3).take(count) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        PixelFormat::BGR888 => {
            for px in pixels.chunks_exact(3).take(count) {
                out.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
        _ => return Err(format!("Unsupported pixel format: {:?}", format)),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bgra_to_rgba_and_png_save() {
        let bgra = vec![
            255, 0, 0, 255, // Blue in BGRA -> RGBA (0,0,255,255)
            0, 255, 0, 255, // Green
            0, 0, 255, 255, // Red
            10, 20, 30, 0, // Alpha forced opaque
        ];
        let rgba = to_rgba_bytes(2, 2, PixelFormat::BGRA8888, &bgra).unwrap();
        assert_eq!(&rgba[0..4], &[0, 0, 255, 255]);
        assert_eq!(&rgba[8..12], &[255, 0, 0, 255]);
        assert_eq!(&rgba[12..16], &[30, 20, 10, 255]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_kwin_target_matcher_escapes_title_and_excludes_hud() {
        let js = kwin_target_matcher_js("Region \"to\" Share");
        assert!(js.contains(r#"var target = "Region \"to\" Share";"#));
        assert!(js.contains("Region to Share Controls"));
        assert!(js.contains("currentDesktopForScreen"));
    }
}
