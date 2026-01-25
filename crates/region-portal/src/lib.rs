//! Portal/PipeWire backend for Wayland screen capture.
//!
//! This crate provides screen capture via the XDG Desktop Portal
//! ScreenCast interface, which is the standard way to capture screens
//! on Wayland compositors (GNOME, KDE Plasma, Sway, Hyprland, etc.).

mod portal;
mod pipewire;
mod stream;
mod dmabuf_import;

pub use portal::{PortalCapture, PortalError, StreamInfo, RestoreToken};
pub use pipewire::{PipeWireStream, PipeWireFrame, StreamState};
pub use stream::PortalBackend;
pub use dmabuf_import::DmaBufImporter;

// Re-export for convenience
pub use region_capture::{CaptureBackend, Capabilities, CaptureError, Frame, Result};
pub use region_core::{Rectangle, PixelFormat};
