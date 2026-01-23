//! XDG Desktop Portal ScreenCast interface.

use std::collections::HashMap;
use thiserror::Error;
use zbus::{Connection, proxy};

/// Portal-specific errors.
#[derive(Error, Debug)]
pub enum PortalError {
    #[error("D-Bus connection failed: {0}")]
    DBusError(String),
    
    #[error("Portal request failed: {0}")]
    RequestFailed(String),
    
    #[error("User cancelled the request")]
    UserCancelled,
    
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

impl From<zbus::Error> for PortalError {
    fn from(err: zbus::Error) -> Self {
        PortalError::DBusError(err.to_string())
    }
}

/// ScreenCast session information.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_handle: String,
}

/// Stream information from Portal.
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub node_id: u32,
    pub width: u32,
    pub height: u32,
}

/// Portal ScreenCast proxy.
#[proxy(
    interface = "org.freedesktop.portal.ScreenCast",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait ScreenCast {
    /// Create a new screencast session.
    fn create_session(&self, options: HashMap<&str, zbus::zvariant::Value<'_>>) 
        -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
    
    /// Select sources for the session.
    fn select_sources(
        &self,
        session_handle: zbus::zvariant::ObjectPath<'_>,
        options: HashMap<&str, zbus::zvariant::Value<'_>>
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
    
    /// Start the screencast stream.
    fn start(
        &self,
        session_handle: zbus::zvariant::ObjectPath<'_>,
        parent_window: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

/// Portal capture interface.
pub struct PortalCapture {
    connection: Connection,
    proxy: ScreenCastProxy<'static>,
    session: Option<SessionInfo>,
}

impl PortalCapture {
    /// Create a new Portal capture interface.
    pub async fn new() -> Result<Self, PortalError> {
        let connection = Connection::session().await?;
        let proxy = ScreenCastProxy::new(&connection).await?;
        
        Ok(Self {
            connection,
            proxy,
            session: None,
        })
    }

    /// Create a screencast session.
    pub async fn create_session(&self) -> Result<SessionInfo, PortalError> {
        let mut options = HashMap::new();
        
        // Generate unique handle
        let handle_token = format!("region_to_share_{}", std::process::id());
        options.insert("handle_token", zbus::zvariant::Value::new(handle_token.as_str()));
        options.insert("session_handle_token", zbus::zvariant::Value::new("region_session"));
        
        let response = self.proxy.create_session(options).await?;
        
        // Wait for Response signal
        // For now, construct session handle from token
        let session_handle = format!("/org/freedesktop/portal/desktop/session/region_session");
        
        Ok(SessionInfo { session_handle })
    }

    /// Select sources to capture.
    pub async fn select_sources(&self, session: &SessionInfo) -> Result<(), PortalError> {
        let mut options = HashMap::new();
        
        // types: 1 = monitor, 2 = window, 4 = virtual
        options.insert("types", zbus::zvariant::Value::new(1u32)); // Monitor
        options.insert("multiple", zbus::zvariant::Value::new(false));
        options.insert("cursor_mode", zbus::zvariant::Value::new(1u32)); // Embedded cursor
        
        let session_path = zbus::zvariant::ObjectPath::try_from(session.session_handle.as_str())
            .map_err(|e| PortalError::InvalidResponse(e.to_string()))?;
        
        self.proxy.select_sources(session_path, options).await?;
        
        Ok(())
    }

    /// Start the screencast stream.
    pub async fn start_stream(&self, session: &SessionInfo) -> Result<StreamInfo, PortalError> {
        let options = HashMap::new();
        
        let session_path = zbus::zvariant::ObjectPath::try_from(session.session_handle.as_str())
            .map_err(|e| PortalError::InvalidResponse(e.to_string()))?;
        
        let _response = self.proxy.start(session_path, "", options).await?;
        
        // In a real implementation, we'd wait for the Start response signal
        // which contains the streams array with node_id and size
        // For now, return mock data
        Ok(StreamInfo {
            node_id: 0, // Will be filled by actual response
            width: 1920,
            height: 1080,
        })
    }

    /// Set cursor mode.
    pub async fn set_cursor_mode(&mut self, _mode: u32) -> Result<(), PortalError> {
        // This would update the session options if the stream is running
        // Mode: 1 = hidden, 2 = embedded, 4 = metadata
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_info() {
        let session = SessionInfo {
            session_handle: "/org/freedesktop/portal/desktop/session/test".to_string(),
        };
        assert!(!session.session_handle.is_empty());
    }

    #[test]
    fn test_stream_info() {
        let stream = StreamInfo {
            node_id: 42,
            width: 1920,
            height: 1080,
        };
        assert_eq!(stream.node_id, 42);
        assert_eq!(stream.width, 1920);
    }

    #[test]
    fn test_portal_error() {
        let err = PortalError::UserCancelled;
        assert_eq!(err.to_string(), "User cancelled the request");
    }
}
