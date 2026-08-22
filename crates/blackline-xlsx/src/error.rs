//! XLSX-domain errors.

use blackline_core::CoreError;

/// Errors produced by the XLSX crate.
#[derive(Debug, thiserror::Error)]
pub enum XlsxError {
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
    /// Any other domain failure.
    #[error("{0}")]
    Invalid(String),
}

impl XlsxError {
    /// Convenience constructor.
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }
}

impl From<String> for XlsxError {
    fn from(msg: String) -> Self {
        Self::Invalid(msg)
    }
}
