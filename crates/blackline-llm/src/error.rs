//! Errors for the blackline-llm pipeline.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// A pipeline failure. CLI maps `Usage` to exit 2 and everything else to 1.
#[derive(Debug, Error)]
pub enum LlmError {
    /// Bad flags, missing author, unsupported file type.
    #[error("{0}")]
    Usage(String),
    /// The instruction could not be turned into ops.
    #[error("{0}")]
    Model(String),
    /// A blackline apply failed.
    #[error("{0}")]
    Apply(String),
    /// Filesystem.
    #[error("{0}")]
    Io(String),
}

impl LlmError {
    /// Prefix used by the CLI to choose exit code 2.
    pub fn is_usage(&self) -> bool {
        matches!(self, Self::Usage(_))
    }

    pub(crate) fn usage(msg: impl ToString) -> Self {
        Self::Usage(msg.to_string())
    }

    pub(crate) fn io_path(path: &std::path::Path, err: io::Error) -> Self {
        Self::Io(format!("{}: {err}", path.display()))
    }

    pub(crate) fn missing(path: PathBuf) -> Self {
        Self::Usage(format!("file not found: {}", path.display()))
    }
}

impl From<io::Error> for LlmError {
    fn from(err: io::Error) -> Self {
        Self::Io(err.to_string())
    }
}
