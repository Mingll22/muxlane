use std::{fmt, io};

use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

/// A stable, redacted error returned across the control-plane boundary.
#[derive(Debug, Error)]
#[error("{code}: {message}")]
pub struct CoreError {
    pub code: &'static str,
    pub message: String,
}

impl CoreError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn storage(_: impl fmt::Display) -> Self {
        Self::new("STORAGE_FAILURE", "a durable storage operation failed")
    }

    pub fn io(_: io::Error) -> Self {
        Self::new("STORAGE_FAILURE", "a controlled filesystem operation failed")
    }
}

impl From<rusqlite::Error> for CoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::storage(error)
    }
}

impl From<io::Error> for CoreError {
    fn from(error: io::Error) -> Self {
        Self::io(error)
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(_: serde_json::Error) -> Self {
        Self::new("INVALID_REQUEST", "invalid structured data")
    }
}
