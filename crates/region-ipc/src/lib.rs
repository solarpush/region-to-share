//! IPC & D-Bus control bus (`io.github.solarpush.RegionToShare`).
//!
//! Acts as the unified control plane linking:
//! - The main GUI / daemon process
//! - The enriched CLI (`region-to-share status`, `stop`, `toggle-record`, `set-region`, ...)
//! - The KDE Plasma 6 Plasmoid (`org.kde.plasma.regiontoshare`)
//! - The GNOME Shell Extension (`region-to-share@solarpush.github.io`)

pub mod protocol;
pub mod socket;

#[cfg(target_os = "linux")]
pub mod dbus_service;

pub use protocol::{
    IpcCommand, IpcResponse, PresetDbusItem, SessionStatusSnapshot, DBUS_INTERFACE, DBUS_NAME,
    DBUS_PATH,
};

#[cfg(target_os = "linux")]
pub use dbus_service::{DbusClient, DbusServiceHandle, SharedSessionState};
