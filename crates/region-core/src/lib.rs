//! Core types and utilities for Region to Share v3.
//!
//! Pure, OS-agnostic domain types shared across every crate in the workspace:
//! - Geometric primitives (`Point`, `Rectangle`, `AspectRatio`)
//! - Pixel formats (`PixelFormat`)
//! - Canonical v3 configuration (`AppConfig`, `RegionPreset`, `RecordingConfig`, ...)
//! - Error types & performance profiling (`FrameProfiler`, `SharedBufferPool`)

pub mod config;
pub mod error;
pub mod geometry;
pub mod performance;

// Re-export commonly used types
pub use config::{
    AppConfig, AudioMode, CaptureBackendKind, CaptureConfig, ContainerFormat, DisplayConfig,
    HardwareEncoder, HudMode, PauseScreenMode, PortalConfig, PortalSelectionStrategy, PresetAction,
    RecordingConfig, RegionPreset, SessionStateKind, UiConfig, UiTheme, VideoCodec,
    CONFIG_SCHEMA_VERSION,
};
pub use error::{ConfigError, CoreError, Result};
pub use geometry::{AspectRatio, PixelFormat, Point, Rectangle};
pub use performance::{BufferPool, FrameProfiler, ProfilerStats, SharedBufferPool};

// Backwards-compatible aliases
pub type Config = AppConfig;
pub type CaptureBackend = CaptureBackendKind;

/// Format the current local timestamp using `%Y`, `%m`, `%d`, `%H`, `%M`, `%S` placeholders.
/// Zero external timezone crate dependencies.
pub fn format_local_timestamp(template: &str) -> String {
    let (year, month, day, hour, min, sec) = current_local_ymdhms();
    template
        .replace("%Y", &format!("{year:04}"))
        .replace("%m", &format!("{month:02}"))
        .replace("%d", &format!("{day:02}"))
        .replace("%H", &format!("{hour:02}"))
        .replace("%M", &format!("{min:02}"))
        .replace("%S", &format!("{sec:02}"))
}

#[cfg(unix)]
fn current_local_ymdhms() -> (i32, u32, u32, u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&secs, &mut tm);
    }
    (
        tm.tm_year + 1900,
        (tm.tm_mon + 1).max(1) as u32,
        tm.tm_mday.max(1) as u32,
        tm.tm_hour.max(0) as u32,
        tm.tm_min.max(0) as u32,
        tm.tm_sec.max(0) as u32,
    )
}

#[cfg(not(unix))]
fn current_local_ymdhms() -> (i32, u32, u32, u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let hour = (rem / 3600) as u32;
    let min = ((rem % 3600) / 60) as u32;
    let sec = (rem % 60) as u32;
    // Civil date from days since 1970-01-01 (Howard Hinnant algorithm)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = (y + i64::from(m <= 2)) as i32;
    (year, m as u32, d as u32, hour, min, sec)
}
