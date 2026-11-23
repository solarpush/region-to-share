//! Screen capture abstraction, zero-copy `FrameHub` fan-out, and OS backends.

pub mod auto;
pub mod backend;
pub mod frame;
pub mod hub;
pub mod stubs;

#[cfg(all(target_os = "linux", feature = "x11"))]
pub mod x11;

// Re-export commonly used types
pub use auto::AutoBackend;
pub use backend::{Capabilities, CaptureBackend, CaptureError, Result};
pub use frame::{Frame, FrameData};
pub use hub::{FrameHub, FrameSubscriber, HubStats};
