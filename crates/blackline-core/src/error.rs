//! Shared error type for package-level operations.

use std::path::PathBuf;

/// Errors produced by the shared OPC / XML layers.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Filesystem I/O failure, annotated with the path involved.
    #[error("I/O error on {path}: {source}")]
    Io {
        /// The path being read or written.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The file is not a readable ZIP/OPC package.
    #[error("not a valid ZIP/OPC package: {0}")]
    Zip(String),

    /// An XML part failed to parse.
    #[error("XML parse error in {part}: {message}")]
    Xml {
        /// Package part name (e.g. `word/document.xml`).
        part: String,
        /// Parser message.
        message: String,
    },

    /// A required package part is missing.
    #[error("missing package part: {0}")]
    MissingPart(String),

    /// A node path did not resolve.
    #[error("XML path not found: {0}")]
    Path(String),

    /// An XML formula or selector failed to parse or evaluate.
    #[error("XML formula error: {0}")]
    Formula(String),

    /// An RFC 5261 XML patch failed to parse or apply.
    #[error("XML patch error: {0}")]
    Patch(String),

    /// An XQuery Update expression failed to parse or compile.
    #[error("XML update error: {0}")]
    Update(String),

    /// Any other invariant violation.
    #[error("{0}")]
    Invalid(String),
}

impl CoreError {
    /// Construct an [`CoreError::Io`] from a path and source error.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.into(),
            source,
        }
    }

    /// Construct an [`CoreError::Xml`] from a part name and message.
    pub fn xml(part: impl Into<String>, message: impl Into<String>) -> Self {
        CoreError::Xml {
            part: part.into(),
            message: message.into(),
        }
    }

    /// Construct an [`CoreError::Invalid`].
    pub fn invalid(msg: impl Into<String>) -> Self {
        CoreError::Invalid(msg.into())
    }

    /// Construct an [`CoreError::Formula`].
    pub fn formula(msg: impl Into<String>) -> Self {
        CoreError::Formula(msg.into())
    }

    /// Construct an [`CoreError::Patch`].
    pub fn patch(msg: impl Into<String>) -> Self {
        CoreError::Patch(msg.into())
    }

    /// Construct an [`CoreError::Update`].
    pub fn update(msg: impl Into<String>) -> Self {
        CoreError::Update(msg.into())
    }
}

impl From<String> for CoreError {
    fn from(msg: String) -> Self {
        CoreError::Invalid(msg)
    }
}
