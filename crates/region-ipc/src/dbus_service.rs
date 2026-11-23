//! Linux Session D-Bus service & client (`io.github.solarpush.RegionToShare`).
//!
//! Consumed natively by:
//! - KDE Plasma 6 Plasmoid (`org.kde.plasma.regiontoshare`) via QML D-Bus / `qdbus`
//! - GNOME Shell Extension (`region-to-share@solarpush.github.io`) via `Gio.DBusProxy`
//! - Region to Share CLI (`region-to-share status|stop|pause|toggle-record|...`)

use crate::{
    IpcCommand, SessionStatusSnapshot, DBUS_INTERFACE, DBUS_NAME, DBUS_PATH,
};
use log::{debug, info};
use parking_lot::RwLock;
use region_core::{Rectangle, SessionStateKind};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection, Proxy};

/// Shared live state exposed as D-Bus properties.
pub type SharedSessionState = Arc<RwLock<SessionStatusSnapshot>>;

struct RegionToShareDbus {
    state: SharedSessionState,
    cmd_tx: UnboundedSender<IpcCommand>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
}

impl RegionToShareDbus {
    fn dispatch(&self, cmd: IpcCommand) {
        let _ = self.cmd_tx.send(cmd);
        (self.on_wake)();
    }
}

#[interface(name = "io.github.solarpush.RegionToShare1")]
impl RegionToShareDbus {
    // ── Methods ───────────────────────────────────────────────────────────────

    async fn show_window(&self) -> zbus::fdo::Result<()> {
        self.dispatch(IpcCommand::Raise);
        Ok(())
    }

    async fn open_settings(&self) -> zbus::fdo::Result<()> {
        self.dispatch(IpcCommand::OpenSettings);
        Ok(())
    }

    async fn start_share(
        &self,
        native_portal: bool,
        preset_or_region: String,
        virtual_cam: bool,
    ) -> zbus::fdo::Result<String> {
        let target = if preset_or_region.trim().is_empty() {
            None
        } else {
            Some(preset_or_region)
        };
        self.dispatch(IpcCommand::StartShare {
            native_portal,
            preset_or_region: target,
            virtual_cam,
        });
        Ok("ok".to_string())
    }

    async fn start_record(
        &self,
        native_portal: bool,
        preset_or_region: String,
        output_path: String,
        also_share: bool,
    ) -> zbus::fdo::Result<String> {
        let target = if preset_or_region.trim().is_empty() {
            None
        } else {
            Some(preset_or_region)
        };
        let out = if output_path.trim().is_empty() {
            None
        } else {
            Some(output_path)
        };
        self.dispatch(IpcCommand::StartRecord {
            native_portal,
            preset_or_region: target,
            output_path: out,
            also_share,
        });
        Ok("ok".to_string())
    }

    async fn take_snapshot(&self, to_clipboard: bool, output_path: String) -> zbus::fdo::Result<String> {
        let out = if output_path.trim().is_empty() {
            None
        } else {
            Some(output_path)
        };
        self.dispatch(IpcCommand::TakeSnapshot {
            to_clipboard,
            output_path: out,
        });
        Ok("ok".to_string())
    }

    async fn toggle_record(&self) -> zbus::fdo::Result<bool> {
        self.dispatch(IpcCommand::ToggleRecord);
        let st = self.state.read().state;
        Ok(matches!(
            st,
            SessionStateKind::Recording | SessionStateKind::SharingAndRecording
        ))
    }

    async fn pause(&self) -> zbus::fdo::Result<()> {
        self.dispatch(IpcCommand::Pause);
        Ok(())
    }

    async fn resume(&self) -> zbus::fdo::Result<()> {
        self.dispatch(IpcCommand::Resume);
        Ok(())
    }

    async fn stop(&self) -> zbus::fdo::Result<()> {
        self.dispatch(IpcCommand::Stop);
        Ok(())
    }

    async fn set_region(&self, x: i32, y: i32, width: u32, height: u32) -> zbus::fdo::Result<()> {
        self.dispatch(IpcCommand::SetRegion {
            region: Rectangle::new(x, y, width, height),
        });
        Ok(())
    }

    async fn get_status_json(&self) -> zbus::fdo::Result<String> {
        let snap = self.state.read().clone();
        serde_json::to_string(&snap)
            .map_err(|e| zbus::fdo::Error::Failed(format!("Serialize failed: {e}")))
    }

    async fn list_presets(&self) -> zbus::fdo::Result<Vec<(String, String, i32, i32, u32, u32)>> {
        let snap = self.state.read();
        Ok(snap
            .presets
            .iter()
            .map(|p| (p.id.clone(), p.name.clone(), p.x, p.y, p.width, p.height))
            .collect())
    }

    // ── Live Properties (read directly by KDE Plasmoid & GNOME Extension) ────

    #[zbus(property)]
    async fn state(&self) -> String {
        self.state.read().state.as_dbus_str().to_string()
    }

    #[zbus(property)]
    async fn active_region(&self) -> (i32, i32, u32, u32) {
        self.state
            .read()
            .active_region
            .map(|r| (r.x, r.y, r.width, r.height))
            .unwrap_or((0, 0, 0, 0))
    }

    #[zbus(property)]
    async fn elapsed_seconds(&self) -> u64 {
        self.state.read().elapsed_seconds
    }

    #[zbus(property)]
    async fn bytes_written(&self) -> u64 {
        self.state.read().bytes_written
    }

    #[zbus(property)]
    async fn current_fps(&self) -> f64 {
        self.state.read().current_fps
    }

    #[zbus(property)]
    async fn virtual_cam_active(&self) -> bool {
        self.state.read().virtual_cam_active
    }

    #[zbus(property)]
    async fn output_file(&self) -> String {
        self.state
            .read()
            .output_file
            .clone()
            .unwrap_or_default()
    }

    // ── Signals ───────────────────────────────────────────────────────────────

    #[zbus(signal, name = "StateChanged")]
    async fn session_state_changed(emitter: &SignalEmitter<'_>, new_state: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn recording_finished(
        emitter: &SignalEmitter<'_>,
        file_path: &str,
        duration_secs: u64,
        size_bytes: u64,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn error_occurred(
        emitter: &SignalEmitter<'_>,
        code: &str,
        message: &str,
    ) -> zbus::Result<()>;
}

/// Active D-Bus server handle kept alive for the lifetime of the app/daemon.
pub struct DbusServiceHandle {
    connection: Connection,
}

impl DbusServiceHandle {
    /// Register `io.github.solarpush.RegionToShare` on the session D-Bus.
    pub async fn start(
        state: SharedSessionState,
        cmd_tx: UnboundedSender<IpcCommand>,
        on_wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, zbus::Error> {
        let iface = RegionToShareDbus {
            state,
            cmd_tx,
            on_wake,
        };
        let connection = Connection::session().await?;
        connection.object_server().at(DBUS_PATH, iface).await?;
        connection.request_name(DBUS_NAME).await?;
        info!("[D-Bus] Registered service {} at {}", DBUS_NAME, DBUS_PATH);
        Ok(Self { connection })
    }

    /// Emit `StateChanged` + property invalidations to wake KDE/GNOME indicators immediately.
    pub async fn emit_state_changed(&self, new_state: SessionStateKind) {
        if let Ok(iface_ref) = self
            .connection
            .object_server()
            .interface::<_, RegionToShareDbus>(DBUS_PATH)
            .await
        {
            let emitter = iface_ref.signal_emitter();
            let _ = RegionToShareDbus::session_state_changed(emitter, new_state.as_dbus_str()).await;
            let guard = iface_ref.get().await;
            let _ = guard.state_changed(emitter).await;
            let _ = guard.elapsed_seconds_changed(emitter).await;
            let _ = guard.bytes_written_changed(emitter).await;
            let _ = guard.current_fps_changed(emitter).await;
        }
    }

    pub async fn emit_recording_finished(&self, path: &str, duration_secs: u64, size_bytes: u64) {
        if let Ok(iface_ref) = self
            .connection
            .object_server()
            .interface::<_, RegionToShareDbus>(DBUS_PATH)
            .await
        {
            let _ = RegionToShareDbus::recording_finished(
                iface_ref.signal_emitter(),
                path,
                duration_secs,
                size_bytes,
            )
            .await;
        }
    }
}

/// D-Bus client used by the CLI (`status`, `stop`, `toggle-record`, `set-region`, ...).
pub struct DbusClient {
    proxy: Proxy<'static>,
}

impl DbusClient {
    pub async fn connect() -> Result<Self, String> {
        let conn = Connection::session()
            .await
            .map_err(|e| format!("Session D-Bus unavailable: {e}"))?;
        let proxy = Proxy::new(&conn, DBUS_NAME, DBUS_PATH, DBUS_INTERFACE)
            .await
            .map_err(|e| format!("Region to Share D-Bus service unavailable: {e}"))?;
        debug!("[D-Bus] Connected client to {}", DBUS_NAME);
        Ok(Self { proxy })
    }

    pub async fn get_status(&self) -> Result<SessionStatusSnapshot, String> {
        let json: String = self
            .proxy
            .call("GetStatusJson", &())
            .await
            .map_err(|e| format!("D-Bus GetStatusJson failed: {e}"))?;
        serde_json::from_str(&json).map_err(|e| format!("Invalid status JSON: {e}"))
    }

    pub async fn send_command(&self, cmd: &IpcCommand) -> Result<String, String> {
        match cmd {
            IpcCommand::Raise => {
                let _: () = self
                    .proxy
                    .call("ShowWindow", &())
                    .await
                    .map_err(|e| e.to_string())?;
                Ok("Window raised".into())
            }
            IpcCommand::OpenSettings => {
                let _: () = self
                    .proxy
                    .call("OpenSettings", &())
                    .await
                    .map_err(|e| e.to_string())?;
                Ok("Settings opened".into())
            }
            IpcCommand::GetStatus => {
                let s = self.get_status().await?;
                serde_json::to_string_pretty(&s).map_err(|e| e.to_string())
            }
            IpcCommand::StartShare {
                native_portal,
                preset_or_region,
                virtual_cam,
            } => {
                let target = preset_or_region.as_deref().unwrap_or("");
                self.proxy
                    .call("StartShare", &(*native_portal, target, *virtual_cam))
                    .await
                    .map_err(|e| e.to_string())
            }
            IpcCommand::StartRecord {
                native_portal,
                preset_or_region,
                output_path,
                also_share,
            } => {
                let target = preset_or_region.as_deref().unwrap_or("");
                let out = output_path.as_deref().unwrap_or("");
                self.proxy
                    .call("StartRecord", &(*native_portal, target, out, *also_share))
                    .await
                    .map_err(|e| e.to_string())
            }
            IpcCommand::TakeSnapshot {
                to_clipboard,
                output_path,
            } => {
                let out = output_path.as_deref().unwrap_or("");
                self.proxy
                    .call("TakeSnapshot", &(*to_clipboard, out))
                    .await
                    .map_err(|e| e.to_string())
            }
            IpcCommand::ToggleRecord => {
                let rec: bool = self
                    .proxy
                    .call("ToggleRecord", &())
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(if rec { "recording" } else { "stopped" }.into())
            }
            IpcCommand::Pause => {
                let _: () = self.proxy.call("Pause", &()).await.map_err(|e| e.to_string())?;
                Ok("paused".into())
            }
            IpcCommand::Resume => {
                let _: () = self.proxy.call("Resume", &()).await.map_err(|e| e.to_string())?;
                Ok("resumed".into())
            }
            IpcCommand::Stop => {
                let _: () = self.proxy.call("Stop", &()).await.map_err(|e| e.to_string())?;
                Ok("stopped".into())
            }
            IpcCommand::SetRegion { region } => {
                let _: () = self
                    .proxy
                    .call("SetRegion", &(region.x, region.y, region.width, region.height))
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(format!("region updated to {region}"))
            }
        }
    }
}
