//! Core types and utilities for Region to Share.
//!
//! This crate provides the fundamental types used throughout the application:
//! - Geometric primitives (Point, Rectangle)
//! - Pixel formats
//! - Configuration structures
//! - Error types
//! - Performance profiling

pub mod config;
pub mod error;
pub mod geometry;
pub mod performance;

// Re-export commonly used types
pub use config::{CaptureBackend, Config};
pub use error::{ConfigError, CoreError, Result};
pub use performance::{FrameProfiler, BufferPool};
pub use geometry::{PixelFormat, Point, Rectangle};
