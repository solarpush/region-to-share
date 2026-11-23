//! Portal + PipeWire backend for Wayland screen capture & virtual camera output.
//!
//! Features:
//! - XDG Desktop Portal `ScreenCast` v6 introspection (`PortalCapabilities`)
//! - Persisted `RestoreToken` (`PersistMode::ExplicitlyRevoked`) to avoid repeated prompts
//! - Native KDE Plasma 6 direct region mode (`zkde_screencast_unstable_v1::stream_region`)
//!   without forcing a second redundant overlay selection
//! - In-callback PipeWire cropping with reusable `SharedBufferPool` & stride-safe parsing
//! - PipeWire Virtual Camera (`Video/Source`, `Camera`) sink for direct Meet/Discord/OBS sharing

mod pipewire;
mod portal;
mod stream;
mod virtual_cam;

pub use pipewire::{PipeWireFrame, PipeWireStream, StreamState};
pub use portal::{
    PortalCapabilities, PortalCapture, PortalError, PortalSessionMode, RestoreToken, StreamInfo,
};
pub use stream::{PortalBackend, PortalBackendOptions};
pub use virtual_cam::{VirtualCameraConfig, VirtualCameraSink};

// Re-export core capture types for convenience
pub use region_capture::{Capabilities, CaptureBackend, CaptureError, Frame, Result};
pub use region_core::{PixelFormat, Rectangle};
