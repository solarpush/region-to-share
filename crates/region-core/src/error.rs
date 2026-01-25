//! Error types for the region-core crate.

use thiserror::Error;

/// Main error type for region-core operations.
#[derive(Error, Debug)]
pub enum CoreError {
    /// Invalid rectangle dimensions.
    #[error("Invalid rectangle: {0}")]
    InvalidRectangle(String),

    /// Invalid point coordinates.
    #[error("Invalid point: {0}")]
    InvalidPoint(String),

    /// Invalid pixel format.
    #[error("Invalid pixel format: {0}")]
    InvalidPixelFormat(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Generic error with custom message.
    #[error("{0}")]
    Custom(String),
}

/// Configuration-specific errors.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Failed to find config directory.
    #[error("Could not determine config directory")]
    NoConfigDir,

    /// Failed to parse config file.
    #[error("Failed to parse config: {0}")]
    ParseError(String),

    /// Failed to write config file.
    #[error("Failed to write config: {0}")]
    WriteError(String),

    /// Invalid config value.
    #[error("Invalid config value for '{key}': {message}")]
    InvalidValue { key: String, message: String },

    /// Missing required config field.
    #[error("Missing required config field: {0}")]
    MissingField(String),
}

/// Result type for region-core operations.
pub type Result<T> = std::result::Result<T, CoreError>;

/// Result type for configuration operations.
pub type ConfigResult<T> = std::result::Result<T, ConfigError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = CoreError::InvalidRectangle("width cannot be zero".to_string());
        assert_eq!(format!("{}", err), "Invalid rectangle: width cannot be zero");
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::InvalidValue {
            key: "fps".to_string(),
            message: "must be positive".to_string(),
        };
        assert_eq!(
            format!("{}", err),
            "Invalid config value for 'fps': must be positive"
        );
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let core_err = CoreError::from(io_err);
        assert!(matches!(core_err, CoreError::Io(_)));
    }

    #[test]
    fn test_error_from_config() {
        let config_err = ConfigError::NoConfigDir;
        let core_err = CoreError::from(config_err);
        assert!(matches!(core_err, CoreError::Config(_)));
    }
}
