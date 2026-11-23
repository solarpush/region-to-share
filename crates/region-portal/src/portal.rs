//! XDG Desktop Portal ScreenCast v6 interface using `ashpd` + direct `zbus`
//! response parsing for reliable `restore_token` persistence.
//!
//! Note: `ashpd 0.13.3` mistakenly marks `Streams::restore_token` with
//! `#[serde(skip_deserializing)]`, causing `response.restore_token()` to always
//! return `None`. Calling `org.freedesktop.portal.ScreenCast.Start` and reading
//! the `org.freedesktop.portal.Request::Response` signal directly via `zbus`
//! guarantees we receive and persist the real `restore_token` on KDE & GNOME!

use ashpd::desktop::screencast::{
    CursorMode, OpenPipeWireRemoteOptions, Screencast, SelectSourcesOptions, SourceType,
};
use ashpd::desktop::{CreateSessionOptions, PersistMode};
use ashpd::enumflags2::BitFlags;
use futures_util::StreamExt;
use log::{debug, info, warn};
use region_platform::DesktopEnvironment;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::fd::{AsRawFd, OwnedFd};
use thiserror::Error;
use zbus::zvariant::{OwnedValue, Value};

/// Portal-specific errors.
#[derive(Error, Debug)]
pub enum PortalError {
    #[error("Portal request failed: {0}")]
    RequestFailed(String),

    #[error("User cancelled the portal dialog")]
    UserCancelled,

    #[error("No streams returned by portal")]
    NoStreams,

    #[error("PipeWire error: {0}")]
    PipeWire(String),
}

impl From<ashpd::Error> for PortalError {
    fn from(err: ashpd::Error) -> Self {
        match err {
            ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled) => {
                PortalError::UserCancelled
            }
            _ => PortalError::RequestFailed(err.to_string()),
        }
    }
}

/// How the portal session is requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PortalSessionMode {
    /// Request `Monitor | Window` so KDE Plasma 6 displays "Rectangular Region",
    /// monitors, and windows natively — and stream the selected source directly
    /// without forcing a second crop overlay.
    NativeDirect,
    /// Request a `Monitor` stream (reusing persisted `RestoreToken` when available)
    /// so Region to Share's own interactive overlay handles cropping, aspect-ratio
    /// locking, magnifier, presets, and live re-cropping.
    #[default]
    MonitorForOverlay,
}

/// Introspected capabilities of `org.freedesktop.portal.ScreenCast` on the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalCapabilities {
    pub available: bool,
    pub version: u32,
    pub supports_monitor: bool,
    pub supports_window: bool,
    pub supports_virtual: bool,
    pub supports_embedded_cursor: bool,
    pub supports_metadata_cursor: bool,
    pub desktop_environment: DesktopEnvironment,
    pub supports_native_region_picker: bool,
}

impl PortalCapabilities {
    pub async fn probe() -> Self {
        let de = DesktopEnvironment::detect();
        let Ok(sc) = Screencast::new().await else {
            return Self {
                available: false,
                version: 0,
                supports_monitor: false,
                supports_window: false,
                supports_virtual: false,
                supports_embedded_cursor: false,
                supports_metadata_cursor: false,
                desktop_environment: de,
                supports_native_region_picker: false,
            };
        };

        let version = sc.version();
        let sources = sc.available_source_types().await.unwrap_or_default();
        let cursors = sc.available_cursor_modes().await.unwrap_or_default();

        Self {
            available: true,
            version,
            supports_monitor: sources.contains(SourceType::Monitor),
            supports_window: sources.contains(SourceType::Window),
            supports_virtual: sources.contains(SourceType::Virtual),
            supports_embedded_cursor: cursors.contains(CursorMode::Embedded),
            supports_metadata_cursor: cursors.contains(CursorMode::Metadata),
            desktop_environment: de,
            supports_native_region_picker: de.has_native_portal_region_picker() && version >= 4,
        }
    }
}

/// Stream metadata returned by the portal.
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub node_id: u32,
    pub width: u32,
    pub height: u32,
    pub position: Option<(i32, i32)>,
    pub source_type: String,
    pub mapping_id: Option<String>,
    pub is_precropped: bool,
}

/// Persisted restore token returned by `org.freedesktop.portal.ScreenCast`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreToken(pub String);

/// Portal capture session holder.
pub struct PortalCapture {
    streams: Vec<StreamInfo>,
    restore_token: Option<RestoreToken>,
    pipewire_fd: Option<OwnedFd>,
}

impl PortalCapture {
    pub async fn new() -> Result<Self, PortalError> {
        Ok(Self {
            streams: Vec::new(),
            restore_token: None,
            pipewire_fd: None,
        })
    }

    /// Create a screencast session using the requested [`PortalSessionMode`].
    pub async fn create_session(
        &mut self,
        restore_token: Option<RestoreToken>,
        include_cursor: bool,
        mode: PortalSessionMode,
    ) -> Result<Vec<StreamInfo>, PortalError> {
        debug!(
            "[Portal] Creating screencast session (cursor={}, mode={:?}, has_token={})",
            include_cursor,
            mode,
            restore_token.is_some()
        );

        let screencast = Screencast::new().await?;
        let cursor_mode = if include_cursor {
            CursorMode::Embedded
        } else {
            CursorMode::Hidden
        };

        let sources: BitFlags<SourceType> = match mode {
            PortalSessionMode::NativeDirect => SourceType::Monitor | SourceType::Window,
            PortalSessionMode::MonitorForOverlay => BitFlags::from(SourceType::Monitor),
        };

        let effective_token = match mode {
            PortalSessionMode::NativeDirect => None,
            PortalSessionMode::MonitorForOverlay => restore_token,
        };

        let persist_mode = match mode {
            PortalSessionMode::NativeDirect => PersistMode::DoNot,
            PortalSessionMode::MonitorForOverlay => PersistMode::ExplicitlyRevoked,
        };

        let session = screencast
            .create_session(CreateSessionOptions::default())
            .await?;

        screencast
            .select_sources(
                &session,
                SelectSourcesOptions::default()
                    .set_cursor_mode(cursor_mode)
                    .set_sources(sources)
                    .set_multiple(false)
                    .set_restore_token(effective_token.as_ref().map(|t| t.0.as_str()))
                    .set_persist_mode(persist_mode),
            )
            .await?
            .response()?;

        debug!("[Portal] Calling Start directly via zbus to preserve restore_token...");
        let (streams, new_token) =
            Self::start_session_raw(&screencast, &session, mode).await?;

        if streams.is_empty() {
            warn!("[Portal] No streams returned by portal");
            return Err(PortalError::NoStreams);
        }

        let pipewire_fd = screencast
            .open_pipe_wire_remote(&session, OpenPipeWireRemoteOptions {})
            .await?;
        debug!("[Portal] Opened PipeWire remote fd={}", pipewire_fd.as_raw_fd());
        self.pipewire_fd = Some(pipewire_fd);

        for stream in &streams {
            info!(
                "[Portal] Stream node={} size={}x{} pos={:?} type={} precropped={}",
                stream.node_id,
                stream.width,
                stream.height,
                stream.position,
                stream.source_type,
                stream.is_precropped
            );
        }

        if let Some(tok) = new_token {
            info!(
                "[Portal] Received persistent restore_token ({} chars) — future sessions skip prompt!",
                tok.len()
            );
            self.restore_token = Some(RestoreToken(tok));
        } else {
            debug!("[Portal] No restore_token returned in Start response");
        }

        self.streams = streams.clone();
        Ok(streams)
    }

    /// Invoke `org.freedesktop.portal.ScreenCast.Start` directly via `zbus` and parse
    /// the raw `(u, a{sv})` `Response` signal so `restore_token` is never lost!
    async fn start_session_raw(
        screencast: &Screencast,
        session: &ashpd::desktop::Session<Screencast>,
        mode: PortalSessionMode,
    ) -> Result<(Vec<StreamInfo>, Option<String>), PortalError> {
        let conn = screencast.connection();
        let unique = conn
            .unique_name()
            .ok_or_else(|| PortalError::RequestFailed("No D-Bus unique name".into()))?;
        let sender = unique.as_str().trim_start_matches(':').replace('.', "_");
        let handle_token = format!("rts_{}", uuid::Uuid::new_v4().simple());
        let expected_req_path = format!(
            "/org/freedesktop/portal/desktop/request/{sender}/{handle_token}"
        );

        let req_proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.portal.Desktop",
            expected_req_path.as_str(),
            "org.freedesktop.portal.Request",
        )
        .await
        .map_err(|e| PortalError::RequestFailed(format!("Request proxy failed: {e}")))?;

        let mut signal_stream = req_proxy
            .receive_signal("Response")
            .await
            .map_err(|e| PortalError::RequestFailed(format!("Response signal subscribe failed: {e}")))?;

        let mut start_opts: HashMap<&str, Value<'_>> = HashMap::new();
        start_opts.insert("handle_token", Value::from(handle_token.as_str()));

        let _: zbus::zvariant::OwnedObjectPath = screencast
            .call("Start", &(session, "", start_opts))
            .await
            .map_err(|e| PortalError::RequestFailed(format!("Start D-Bus call failed: {e}")))?;

        let msg = signal_stream
            .next()
            .await
            .ok_or_else(|| PortalError::RequestFailed("Portal Response signal stream ended".into()))?;

        let (response_code, results): (u32, HashMap<String, OwnedValue>) = msg
            .body()
            .deserialize()
            .map_err(|e| PortalError::RequestFailed(format!("Failed to decode Start response: {e}")))?;

        match response_code {
            0 => {}
            1 => return Err(PortalError::UserCancelled),
            code => {
                return Err(PortalError::RequestFailed(format!(
                    "Portal returned error response code {code}"
                )));
            }
        }

        // Extract restore_token cleanly!
        let restore_token: Option<String> = results
            .get("restore_token")
            .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
            .filter(|s| !s.is_empty());

        // Extract streams: a(ua{sv})
        type RawStreams = Vec<(u32, HashMap<String, OwnedValue>)>;
        let raw_streams: RawStreams = results
            .get("streams")
            .and_then(|v| RawStreams::try_from(v.try_clone().ok()?).ok())
            .unwrap_or_default();

        let streams = raw_streams
            .into_iter()
            .map(|(node_id, props)| {
                let size: Option<(i32, i32)> = props
                    .get("size")
                    .and_then(|v| <(i32, i32)>::try_from(v.try_clone().ok()?).ok());
                let position: Option<(i32, i32)> = props
                    .get("position")
                    .and_then(|v| <(i32, i32)>::try_from(v.try_clone().ok()?).ok());
                let source_type_u32: Option<u32> = props
                    .get("source_type")
                    .and_then(|v| u32::try_from(v.try_clone().ok()?).ok());
                let mapping_id: Option<String> = props
                    .get("mapping_id")
                    .and_then(|v| String::try_from(v.try_clone().ok()?).ok());

                let source_type = match source_type_u32 {
                    Some(1) => "monitor",
                    Some(2) => "window",
                    Some(4) => "virtual",
                    _ => "unknown",
                };
                let is_precropped = matches!(mode, PortalSessionMode::NativeDirect)
                    || matches!(source_type_u32, Some(2 | 4));

                StreamInfo {
                    node_id,
                    width: size.map(|(w, _)| w.max(1) as u32).unwrap_or(1920),
                    height: size.map(|(_, h)| h.max(1) as u32).unwrap_or(1080),
                    position,
                    source_type: source_type.to_string(),
                    mapping_id,
                    is_precropped,
                }
            })
            .collect();

        Ok((streams, restore_token))
    }

    pub fn restore_token(&self) -> Option<&RestoreToken> {
        self.restore_token.as_ref()
    }

    pub fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    pub fn node_id(&self) -> Option<u32> {
        self.streams.first().map(|s| s.node_id)
    }

    pub fn pipewire_fd(&self) -> Option<i32> {
        self.pipewire_fd.as_ref().map(|fd| fd.as_raw_fd())
    }

    pub fn take_pipewire_fd(&mut self) -> Option<OwnedFd> {
        self.pipewire_fd.take()
    }

    pub fn is_active(&self) -> bool {
        !self.streams.is_empty()
    }

    pub async fn close(&mut self) -> Result<(), PortalError> {
        self.streams.clear();
        self.pipewire_fd = None;
        Ok(())
    }
}
