//! Typed core error enum for `studio_core`.

use thiserror::Error;

/// Top-level error type for core engine operations.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("record not found: {0}")]
    NotFound(String),

    #[error("invalid argument: {0}")]
    InvalidInput(String),

    #[error("external process failed: {0}")]
    Process(String),

    #[error("operation cancelled")]
    Cancelled,
}

