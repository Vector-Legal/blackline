//! PPTX-domain errors.

use blackline_core::CoreError;

/// Errors produced by the PPTX crate.
#[derive(Debug, thiserror::Error)]
pub enum PptxError {
    /// Shared OPC / XML failure.
    #[error(transparent)]
    Core(#[from] CoreError),
    /// An edit operation failed.
    #[error("op {index} ({op}) failed: {reason}")]
    OpFailed {
        /// Index.
        index: usize,
        /// Name.
        op: String,
        /// Reason.
        reason: String,
    },
    /// Other.
    #[error("{0}")]
    Invalid(String),
}

impl PptxError {
    /// Convenience.
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }
}

impl From<String> for PptxError {
    fn from(msg: String) -> Self {
        Self::Invalid(msg)
    }
}
