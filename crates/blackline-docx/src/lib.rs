//! # blackline-docx
//!
//! Read, search, edit, redline, track, create, and validate DOCX documents.
//! The XML in the package is the document — nothing is converted to HTML.
//!
//! ```no_run
//! use blackline_docx::{Docx, SearchQuery};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let doc = Docx::from_paragraphs(&["Hello", "World"])?;
//! let hits = doc.search(&SearchQuery::new("Hello"))?;
//! assert_eq!(hits.total, 1);
//! # Ok(())
//! # }
//! ```

mod body;
mod check;
mod comment;
mod create;
mod document;
mod edit;
mod error;
mod hyperlink;
mod info;
mod ops;
mod redline;
mod revision;
mod search;
mod splice;
mod style;
mod text;
mod track;

pub use blackline_core::diff::Granularity;
pub use check::DocxHealth;
pub use comment::CommentThread;
pub use create::{CreateSpec, ParaSpec, RunSpec, SectionSpec, TableSpec};
pub use document::{
    redline, track_redline, Docx, EditBuilder, EditOutcome, TrackBuilder, TrackOutcome,
};
pub use edit::{EditOptions, EditReport, OpReport};
pub use error::DocxError;
pub use hyperlink::Hyperlink;
pub use info::DocxInfo;
pub use ops::EditOp;
pub use revision::TrackedChange;
pub use search::{SearchHit, SearchQuery, SearchResults, DEFAULT_LIMIT};
pub use style::{ParaProps, RunProps};
pub use text::{OutlineEntry, ViewLine};
pub use track::{TrackOp, TrackOpReport, TrackOptions, TrackReport};
