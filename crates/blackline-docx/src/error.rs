//! DOCX-domain errors.

use blackline_core::CoreError;

/// Errors produced by the DOCX crate.
#[derive(Debug, thiserror::Error)]
pub enum DocxError {
    /// Shared OPC / XML failure.
    #[error(transparent)]
    Core(#[from] CoreError),

    /// An edit operation failed.
    #[error("op {index} ({op}) failed: {reason}")]
    OpFailed {
        /// Zero-based op index.
        index: usize,
        /// Op name.
        op: String,
        /// Why it failed.
        reason: String,
    },

    /// Tracked changes or comments were requested without an author.
    #[error(
        "author required: tracked changes and comments must carry an explicit author (pass --author or set BLACKLINE_AUTHOR)"
    )]
    AuthorRequired,

    /// Any other domain failure.
    #[error("{0}")]
    Invalid(String),
}

impl DocxError {
    /// Convenience constructor.
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }
}

impl From<String> for DocxError {
    fn from(msg: String) -> Self {
        Self::Invalid(msg)
    }
}
