//! Document metrics.

use serde::Serialize;

use blackline_core::package::Package;

use crate::body::{heading_level, visible_indices, BodyKind};
use crate::comment::list_comments;
use crate::error::DocxError;
use crate::revision::list_changes;
use crate::text::view_lines;

/// Preflight counts.
#[derive(Debug, Clone, Serialize)]
pub struct DocxInfo {
    /// Source file name, when known.
    pub file: String,
    /// Package size in bytes.
    pub file_size_bytes: usize,
    /// Visible body elements.
    pub paragraphs: usize,
    /// Tables.
    pub tables: usize,
    /// Headings.
    pub headings: usize,
    /// Tracked-change counts.
    pub tracked_changes: ChangeCounts,
    /// Comment counts.
    pub comments: CommentCounts,
}

/// Tracked-change tallies.
#[derive(Debug, Clone, Serialize)]
pub struct ChangeCounts {
    /// Total ins+del.
    pub total: usize,
    /// Insertions.
    pub insertions: usize,
    /// Deletions.
    pub deletions: usize,
}

/// Comment tallies.
#[derive(Debug, Clone, Serialize)]
pub struct CommentCounts {
    /// Total comments.
    pub total: usize,
}

/// Inspect a package.
pub fn inspect(pkg: &Package, file: &str) -> Result<DocxInfo, DocxError> {
    let main = pkg.main_document_part()?;
    let doc = pkg.part_xml(&main)?;
    let vis = visible_indices(&doc);
    let mut paragraphs = 0;
    let mut tables = 0;
    for (_, k) in &vis {
        match k {
            BodyKind::Paragraph | BodyKind::Sdt => paragraphs += 1,
            BodyKind::Table => tables += 1,
        }
    }
    let headings = view_lines(&doc, false)?
        .iter()
        .filter(|l| l.heading.is_some())
        .count();
    let changes = list_changes(&doc.root);
    let insertions = changes.iter().filter(|c| c.kind == "insert").count();
    let deletions = changes.iter().filter(|c| c.kind == "delete").count();
    let comments = list_comments(pkg)?;
    let size: usize = pkg.iter().map(|(_, b)| b.len()).sum();
    let _ = heading_level;
    Ok(DocxInfo {
        file: file.to_string(),
        file_size_bytes: size,
        paragraphs,
        tables,
        headings,
        tracked_changes: ChangeCounts {
            total: changes.len(),
            insertions,
            deletions,
        },
        comments: CommentCounts {
            total: comments.len(),
        },
    })
}
