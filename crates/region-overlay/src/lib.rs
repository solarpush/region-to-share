//! Overlay capabilities, interactive selection geometry (8-handle resize,
//! aspect-ratio locking, grid snapping), Wayland `zwlr_layer_shell_v1` probing,
//! and live click-through capture border indicator.

pub mod border;
pub mod selection;

pub use border::{BorderStyle, CaptureBorderHandle};
pub use selection::{ResizeHandle, SelectionEditor};

use region_platform::{DesktopEnvironment, DisplayServer};
use serde::{Deserialize, Serialize};

/// Detected overlay capabilities on the active display server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayCapabilities {
    pub display_server: DisplayServer,
    pub desktop_environment: DesktopEnvironment,
    /// True when the Wayland compositor advertises `zwlr_layer_shell_v1`
    /// (KDE Plasma 6, Sway, Hyprland, COSMIC).
    pub wlr_layer_shell_available: bool,
    /// True when click-through live capture borders are natively supported.
    pub supports_clickthrough_border: bool,
}

impl OverlayCapabilities {
    pub fn probe() -> Self {
        let display_server = DisplayServer::detect();
        let desktop_environment = DesktopEnvironment::detect();
        let wlr_layer_shell_available = if display_server.is_wayland() {
            probe_wlr_layer_shell()
        } else {
            false
        };
        let supports_clickthrough_border = match display_server {
            DisplayServer::X11 => true,
            DisplayServer::Wayland => wlr_layer_shell_available,
            DisplayServer::MacOsQuartz | DisplayServer::WindowsDwm => true,
            DisplayServer::Unknown => false,
        };

        Self {
            display_server,
            desktop_environment,
            wlr_layer_shell_available,
            supports_clickthrough_border,
        }
    }
}

#[cfg(target_os = "linux")]
fn probe_wlr_layer_shell() -> bool {
    use wayland_client::protocol::wl_registry;
    use wayland_client::{Connection, Dispatch, QueueHandle};

    struct RegistryProbe {
        found_layer_shell: bool,
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for RegistryProbe {
        fn event(
            state: &mut Self,
            _: &wl_registry::WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global { interface, .. } = event {
                if interface == "zwlr_layer_shell_v1" {
                    state.found_layer_shell = true;
                }
            }
        }
    }

    let Ok(conn) = Connection::connect_to_env() else {
        return false;
    };
    let display = conn.display();
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = display.get_registry(&qh, ());
    let mut state = RegistryProbe {
        found_layer_shell: false,
    };
    let _ = queue.roundtrip(&mut state);
    state.found_layer_shell
}

#[cfg(not(target_os = "linux"))]
fn probe_wlr_layer_shell() -> bool {
    false
}
