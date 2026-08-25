//! Office format detection. PDF and Markdown are out of scope: blackline
//! edits the XML that lives in an OOXML package.

use std::path::Path;

use serde::Serialize;

use super::error::AiError;

/// The three packages blackline can edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// WordprocessingML.
    Docx,
    /// SpreadsheetML.
    Xlsx,
    /// PresentationML.
    Pptx,
}

impl Format {
    /// Detect from the file extension. Unknown types get a usage error that
    /// names the supported set and explicitly refuses PDF / Markdown.
    pub fn from_path(path: &Path) -> Result<Self, AiError> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "docx" => Ok(Self::Docx),
            "xlsx" => Ok(Self::Xlsx),
            "pptx" => Ok(Self::Pptx),
            "pdf" | "md" | "markdown" | "html" | "htm" | "txt" => Err(AiError::usage(format!(
                "bl ai / bl llm reads native Office files (.docx .xlsx .pptx). \
                 It does not convert .{ext}. Run blackline on the OOXML package."
            ))),
            "" => Err(AiError::usage(
                "file has no extension; expected .docx, .xlsx, or .pptx",
            )),
            other => Err(AiError::usage(format!(
                "unsupported .{other}; expected .docx, .xlsx, or .pptx"
            ))),
        }
    }

    /// Lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
        }
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
