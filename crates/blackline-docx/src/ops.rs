//! JSON edit-operation contract.

use serde::Deserialize;

use crate::style::{ParaProps, RunProps};

/// A single edit operation.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op")]
pub enum EditOp {
    /// Replace `old` with `new` in a body element.
    #[serde(rename = "replace")]
    Replace {
        /// 1-based view index.
        #[serde(default)]
        index: Option<usize>,
        /// Text to find.
        old: String,
        /// Replacement text.
        new: String,
        /// Find the element by content instead of index.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
    },
    /// Insert a paragraph before or after an anchor.
    #[serde(rename = "insert")]
    Insert {
        /// Anchor view index.
        #[serde(default)]
        index: Option<usize>,
        /// `"before"` or `"after"`.
        #[serde(default = "default_after")]
        position: String,
        /// Content-match anchor.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
        /// Paragraph text.
        #[serde(default)]
        text: Option<String>,
        /// Alias for `text`.
        #[serde(default)]
        content: Option<String>,
        /// Paragraph style id.
        #[serde(default)]
        style: Option<String>,
        /// Paragraph properties.
        #[serde(flatten)]
        para_props: ParaProps,
    },
    /// Delete a body element (or a range).
    #[serde(rename = "delete")]
    Delete {
        /// View index.
        #[serde(default)]
        index: Option<usize>,
        /// Inclusive `(from, to)` view-index range.
        #[serde(default)]
        range: Option<(usize, usize)>,
        /// Content-match target.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
    },
    /// Delete a run whose text equals `text`.
    #[serde(rename = "delete_run")]
    DeleteRun {
        /// View index.
        #[serde(default)]
        index: Option<usize>,
        /// Content-match target.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
        /// Run text to delete.
        text: String,
    },
    /// Apply run/paragraph formatting to an element.
    #[serde(rename = "format")]
    Format {
        /// View index.
        index: usize,
        /// Run properties.
        #[serde(flatten)]
        run_props: RunProps,
        /// Paragraph properties.
        #[serde(flatten)]
        para_props: ParaProps,
    },
    /// Insert a table row.
    #[serde(rename = "table_insert_row")]
    TableInsertRow {
        /// Table view index.
        index: usize,
        /// 1-based row position.
        row_index: usize,
        /// `"before"` or `"after"`.
        #[serde(default = "default_after")]
        position: String,
        /// Cell texts.
        cells: Vec<String>,
    },
    /// Delete a table row.
    #[serde(rename = "table_delete_row")]
    TableDeleteRow {
        /// Table view index.
        index: usize,
        /// 1-based row.
        row_index: usize,
    },
    /// Insert a comment anchored on `anchor` text.
    #[serde(rename = "insert_comment")]
    InsertComment {
        /// View index.
        #[serde(default)]
        index: Option<usize>,
        /// Content-match target.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
        /// Text the comment range wraps.
        #[serde(default)]
        anchor: Option<String>,
        /// Comment body.
        text: String,
        /// Author override for this comment.
        #[serde(default)]
        author: Option<String>,
    },
    /// Delete a comment by id.
    #[serde(rename = "delete_comment")]
    DeleteComment {
        /// Comment `w:id`.
        id: String,
    },
    /// Accept one tracked change by id.
    #[serde(rename = "accept_change")]
    AcceptChange {
        /// `w:id` of the `w:ins` / `w:del`.
        id: String,
    },
    /// Reject one tracked change by id.
    #[serde(rename = "reject_change")]
    RejectChange {
        /// `w:id` of the `w:ins` / `w:del`.
        id: String,
    },
    /// Accept every tracked change, optionally filtered by author.
    #[serde(rename = "accept_all")]
    AcceptAll {
        /// Restrict to this author.
        #[serde(default)]
        author: Option<String>,
    },
    /// Reject every tracked change, optionally filtered by author.
    #[serde(rename = "reject_all")]
    RejectAll {
        /// Restrict to this author.
        #[serde(default)]
        author: Option<String>,
    },
    /// Create or retarget a hyperlink (`http:`, `mailto:`, …).
    #[serde(rename = "set_hyperlink")]
    SetHyperlink {
        /// View index.
        #[serde(default)]
        index: Option<usize>,
        /// Content-match target.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
        /// Display text to wrap or retarget.
        text: String,
        /// Target URL.
        url: String,
    },
}

fn default_after() -> String {
    "after".into()
}

impl EditOp {
    /// Stable op name.
    pub fn name(&self) -> &'static str {
        match self {
            EditOp::Replace { .. } => "replace",
            EditOp::Insert { .. } => "insert",
            EditOp::Delete { .. } => "delete",
            EditOp::DeleteRun { .. } => "delete_run",
            EditOp::Format { .. } => "format",
            EditOp::TableInsertRow { .. } => "table_insert_row",
            EditOp::TableDeleteRow { .. } => "table_delete_row",
            EditOp::InsertComment { .. } => "insert_comment",
            EditOp::DeleteComment { .. } => "delete_comment",
            EditOp::AcceptChange { .. } => "accept_change",
            EditOp::RejectChange { .. } => "reject_change",
            EditOp::AcceptAll { .. } => "accept_all",
            EditOp::RejectAll { .. } => "reject_all",
            EditOp::SetHyperlink { .. } => "set_hyperlink",
        }
    }

    /// True when this op needs an author (comments, or any tracked batch).
    pub fn needs_author(&self) -> bool {
        matches!(self, EditOp::InsertComment { .. })
    }
}
