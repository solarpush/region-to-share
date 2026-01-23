//! XDG Desktop Portal ScreenCast interface using ashpd.
//!
//! Compatible with:
//! - GNOME (mutter)
//! - KDE Plasma (kwin)
//! - Wlroots-based (sway, hyprland, etc.)
//! - Any compositor implementing xdg-desktop-portal

use ashpd::desktop::screencast::{CursorMode, SourceType, Screencast};
use ashpd::desktop::PersistMode;
use ashpd::WindowIdentifier;
use thiserror::Error;

/// Portal-specific errors.
#[derive(Error, Debug)]
pub enum PortalError {
    #[error("Portal request failed: {0}")]
    RequestFailed(String),
    
    #[error("User cancelled the request")]
    UserCancelled,
    
    #[error("No streams available")]
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

/// Stream information from Portal.
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub node_id: u32,
    pub width: u32,
    pub height: u32,
    pub source_type: String,
}

/// Restore token for persistent permissions (avoid re-asking user).
#[derive(Debug, Clone)]
pub struct RestoreToken(pub String);

/// Portal capture session.
pub struct PortalCapture {
    streams: Vec<StreamInfo>,
    restore_token: Option<RestoreToken>,
}

impl PortalCapture {
    /// Create a new Portal capture interface.
    pub async fn new() -> Result<Self, PortalError> {
        Ok(Self {
            streams: Vec::new(),
            restore_token: None,
        })
    }

    /// Create a screencast session with user permission dialog.
    pub async fn create_session(
        &mut self,
        restore_token: Option<RestoreToken>,
        include_cursor: bool,
    ) -> Result<Vec<StreamInfo>, PortalError> {
        let screencast = Screencast::new().await?;
        
        let cursor_mode = if include_cursor {
            CursorMode::Embedded
        } else {
            CursorMode::Hidden
        };

        let session = screencast.create_session().await?;
        let persist_mode = PersistMode::Application;
        
        screencast
            .select_sources(
                &session,
                cursor_mode,
                SourceType::Monitor | SourceType::Window,
                false,
                restore_token.as_ref().map(|t| t.0.as_str()),
                persist_mode,
            )
            .await?;

        let response = screencast
            .start(&session, &WindowIdentifier::default())
            .await?
            .response()?;

        let streams: Vec<StreamInfo> = response
            .streams()
            .iter()
            .map(|s| StreamInfo {
                node_id: s.pipe_wire_node_id(),
                width: s.size().map(|(w, _)| w as u32).unwrap_or(1920),
                height: s.size().map(|(_, h)| h as u32).unwrap_or(1080),
                source_type: match s.source_type() {
                    Some(SourceType::Monitor) => "monitor".to_string(),
                    Some(SourceType::Window) => "window".to_string(),
                    Some(SourceType::Virtual) => "virtual".to_string(),
                    _ => "unknown".to_string(),
                },
            })
            .collect();

        if streams.is_empty() {
            return Err(PortalError::NoStreams);
        }

        if let Some(token) = response.restore_token() {
            self.restore_token = Some(RestoreToken(token.to_string()));
        }

        self.streams = streams.clone();

        Ok(streams)
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

    pub fn is_active(&self) -> bool {
        !self.streams.is_empty()
    }

    pub async fn close(&mut self) -> Result<(), PortalError> {
        self.streams.clear();
        Ok(())
    }
}
