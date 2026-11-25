//! Serializable IPC error type for Tauri commands.

use serde::Serialize;
use thiserror::Error;

/// Error returned across the Tauri IPC boundary.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum CommandError {
    #[error("{0}")]
    Core(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("cancelled")]
    Cancelled,
}

impl From<studio_core::CoreError> for CommandError {
    fn from(err: studio_core::CoreError) -> Self {
        match err {
            studio_core::CoreError::NotFound(msg) => Self::NotFound(msg),
            studio_core::CoreError::InvalidInput(msg) => Self::InvalidInput(msg),
            studio_core::CoreError::Cancelled => Self::Cancelled,
            other => Self::Core(other.to_string()),
        }
    }
}

impl From<subtitle_render::RenderError> for CommandError {
    fn from(err: subtitle_render::RenderError) -> Self {
        Self::Core(err.to_string())
    }
}
