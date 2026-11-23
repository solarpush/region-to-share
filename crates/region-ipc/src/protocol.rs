//! Shared IPC / D-Bus protocol messages and status structures.

use region_core::{Rectangle, SessionStateKind};
use serde::{Deserialize, Serialize};

pub const DBUS_NAME: &str = "io.github.solarpush.RegionToShare";
pub const DBUS_PATH: &str = "/io/github/solarpush/RegionToShare";
pub const DBUS_INTERFACE: &str = "io.github.solarpush.RegionToShare1";

/// Lightweight preset summary exposed over D-Bus / IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetDbusItem {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Real-time snapshot of the active session (returned by `status --json` and D-Bus properties).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatusSnapshot {
    pub state: SessionStateKind,
    pub active_region: Option<Rectangle>,
    pub elapsed_seconds: u64,
    pub bytes_written: u64,
    pub current_fps: f64,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub virtual_cam_active: bool,
    pub output_file: Option<String>,
    pub backend_name: String,
    pub presets: Vec<PresetDbusItem>,
}

impl Default for SessionStatusSnapshot {
    fn default() -> Self {
        Self {
            state: SessionStateKind::Idle,
            active_region: None,
            elapsed_seconds: 0,
            bytes_written: 0,
            current_fps: 0.0,
            cpu_percent: 0.0,
            memory_mb: 0.0,
            virtual_cam_active: false,
            output_file: None,
            backend_name: "idle".to_string(),
            presets: Vec::new(),
        }
    }
}

/// Commands dispatched from CLI / KDE Plasmoid / GNOME Extension to the running app/daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcCommand {
    Raise,
    OpenSettings,
    GetStatus,
    StartShare {
        native_portal: bool,
        preset_or_region: Option<String>,
        virtual_cam: bool,
    },
    StartRecord {
        native_portal: bool,
        preset_or_region: Option<String>,
        output_path: Option<String>,
        also_share: bool,
    },
    TakeSnapshot {
        to_clipboard: bool,
        output_path: Option<String>,
    },
    ToggleRecord,
    Pause,
    Resume,
    Stop,
    SetRegion {
        region: Rectangle,
    },
}

/// Response returned over socket IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IpcResponse {
    Ok {
        message: String,
    },
    Status(SessionStatusSnapshot),
    Error {
        message: String,
    },
}
