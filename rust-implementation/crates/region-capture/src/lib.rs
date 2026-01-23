//! Screen capture abstraction and backends.
//!
//! This crate provides a unified interface for capturing screen regions
//! across different platforms and backends (X11, Wayland/Portal, PipeWire).

pub mod backend;
pub mod frame;

#[cfg(feature = "x11")]
pub mod x11;

pub mod auto;

// Re-export commonly used types
pub use backend::{Capabilities, CaptureBackend, CaptureError, Result};
pub use frame::{Frame, FrameData};
pub use auto::AutoBackend;
