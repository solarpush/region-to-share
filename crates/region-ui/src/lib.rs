//! Region to Share v3 GUI library (`eframe`/`egui` + bespoke Design System + live recording & IPC).

pub mod app;
pub mod hud;
pub mod icons;
pub mod pause_screen;
pub mod selector;
pub mod settings_view;
pub mod theme;
pub mod widgets;

rust_i18n::i18n!("locales", fallback = "en");

pub use app::{run_gui, LaunchIntent};
