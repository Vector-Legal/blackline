//! Public façade: [`Docx`].

use std::path::{Path, PathBuf};

use blackline_core::diff::Granularity;
use blackline_core::package::Package;

use crate::check::{self, DocxHealth};
use crate::comment::{list_comments, CommentThread};
use crate::create::{self, CreateSpec};
use crate::edit::{self, EditOptions, EditReport};
use crate::error::DocxError;
use crate::info::{self, DocxInfo};
use crate::ops::EditOp;
use crate::revision::{list_changes, TrackedChange};
use crate::search::{self, SearchQuery, SearchResults};
use crate::text::{self, OutlineEntry, ViewLine};
use crate::track::{self, TrackOp, TrackOptions, TrackReport};

/// An open DOCX document.
pub struct Docx {
    pkg: Package,
    path: Option<PathBuf>,
}

impl Docx {
    /// Open a document from a path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DocxError> {
        let path = path.as_ref();
        let pkg = Package::open(path)?;
        let _ = pkg.main_document_part()?;
        Ok(Self {
            pkg,
            path: Some(path.to_path_buf()),
        })
    }

    /// Open a document from package bytes.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, DocxError> {
        let pkg = Package::from_bytes(bytes.as_ref())?;
        let _ = pkg.main_document_part()?;
        Ok(Self { pkg, path: None })
    }

    /// Open from an in-memory package.
    pub fn from_package(pkg: Package) -> Result<Self, DocxError> {
        let _ = pkg.main_document_part()?;
        Ok(Self { pkg, path: None })
    }

    /// Create from a JSON spec.
    pub fn create(spec: &CreateSpec) -> Result<Self, DocxError> {
        Ok(Self {
            pkg: create::create(spec)?,
            path: None,
        })
    }

    /// Create from a list of paragraph strings.
    pub fn from_paragraphs(texts: &[&str]) -> Result<Self, DocxError> {
        Ok(Self {
            pkg: create::from_paragraphs(texts)?,
            path: None,
        })
    }

    /// The underlying OPC package.
    pub fn package(&self) -> &Package {
        &self.pkg
    }

    /// Mutable package (escape hatch for tree-level edits).
    pub fn package_mut(&mut self) -> &mut Package {
        &mut self.pkg
    }

    /// Source path, when opened from a file.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Numbered view lines.
    pub fn view(&self, raw: bool) -> Result<Vec<ViewLine>, DocxError> {
        let main = self.pkg.main_document_part()?;
        let doc = self.pkg.part_xml(&main)?;
        text::view_lines(&doc, raw)
    }

    /// `{index}| text` lines.
    pub fn text_lines(&self) -> Result<Vec<String>, DocxError> {
        Ok(text::render_view(&self.view(false)?))
    }

    /// Visible body text, one view line per newline.
    pub fn visible_text(&self) -> Result<String, DocxError> {
        Ok(self
            .view(false)?
            .into_iter()
            .map(|l| l.text)
            .collect::<Vec<_>>()
            .join("\n"))
    }

    /// Heading outline.
    pub fn outline(&self) -> Result<Vec<OutlineEntry>, DocxError> {
        let main = self.pkg.main_document_part()?;
        let doc = self.pkg.part_xml(&main)?;
        text::outline(&doc)
    }

    /// Bounded search.
    pub fn search(&self, query: &SearchQuery) -> Result<SearchResults, DocxError> {
        let main = self.pkg.main_document_part()?;
        let doc = self.pkg.part_xml(&main)?;
        search::search(&doc, query)
    }

    /// Preflight metrics.
    pub fn info(&self) -> Result<DocxInfo, DocxError> {
        let name = self
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        info::inspect(&self.pkg, &name)
    }

    /// Tracked changes.
    pub fn changes(&self) -> Result<Vec<TrackedChange>, DocxError> {
        let main = self.pkg.main_document_part()?;
        let doc = self.pkg.part_xml(&main)?;
        Ok(list_changes(&doc.root))
    }

    /// Comment threads.
    pub fn comments(&self) -> Result<Vec<CommentThread>, DocxError> {
        list_comments(&self.pkg)
    }

    /// Start a track / redline batch (multi-author, surgical, minimized).
    pub fn track(&self, ops: impl IntoIterator<Item = TrackOp>) -> TrackBuilder<'_> {
        TrackBuilder {
            doc: self,
            ops: ops.into_iter().collect(),
            options: TrackOptions::default(),
        }
    }

    /// Start an edit batch.
    pub fn edit(&self, ops: impl IntoIterator<Item = EditOp>) -> EditBuilder<'_> {
        EditBuilder {
            doc: self,
            ops: ops.into_iter().collect(),
            options: EditOptions::default(),
        }
    }

    /// Header and footer part names.
    pub fn story_parts(&self) -> Vec<String> {
        crate::edit::story_parts(&self.pkg)
    }

    /// Numbered view of a story part (`word/header2.xml`, …).
    pub fn story_view(&self, part: &str) -> Result<Vec<ViewLine>, DocxError> {
        let doc = self.pkg.part_xml(part)?;
        text::view_lines(&doc, false)
    }

    /// External hyperlinks in the main document.
    pub fn hyperlinks(&self) -> Result<Vec<crate::hyperlink::Hyperlink>, DocxError> {
        let main = self.pkg.main_document_part()?;
        crate::hyperlink::list_hyperlinks(&self.pkg, &main)
    }

    /// Package health check.
    pub fn check(&self) -> Result<DocxHealth, DocxError> {
        check::check(&self.pkg, None)
    }

    /// Health check against an original (redline integrity).
    pub fn check_against(&self, original: &Docx) -> Result<DocxHealth, DocxError> {
        check::check(&self.pkg, Some(&original.pkg))
    }

    /// Save to a path.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), DocxError> {
        self.pkg.save(path)?;
        Ok(())
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, DocxError> {
        Ok(self.pkg.to_bytes()?)
    }
}

/// Builder for an edit batch.
pub struct EditBuilder<'a> {
    doc: &'a Docx,
    ops: Vec<EditOp>,
    options: EditOptions,
}

impl EditBuilder<'_> {
    /// Track changes under `author`.
    pub fn tracked(mut self, author: impl Into<String>) -> Self {
        self.options.tracked = true;
        self.options.author = Some(author.into());
        self
    }

    /// Set the author (comments, or tracked).
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.options.author = Some(author.into());
        self
    }

    /// Redline granularity.
    pub fn granularity(mut self, g: Granularity) -> Self {
        self.options.granularity = g;
        self
    }

    /// Best-effort mode.
    pub fn lenient(mut self) -> Self {
        self.options.lenient = true;
        self
    }

    /// Validate without writing.
    pub fn dry_run(mut self) -> Self {
        self.options.dry_run = true;
        self
    }

    /// Apply this batch to a story part (`word/header2.xml`, `"header"`, `"footer"`).
    pub fn part(mut self, part: impl Into<String>) -> Self {
        self.options.part = Some(part.into());
        self
    }

    /// Apply the batch.
    pub fn apply(self) -> Result<EditOutcome, DocxError> {
        let mut pkg = self.doc.pkg.clone();
        let report = edit::apply(&mut pkg, &self.ops, &self.options)?;
        if self.options.dry_run {
            return Ok(EditOutcome {
                report,
                document: None,
            });
        }
        if report.failed > 0 && !self.options.lenient {
            // apply() already errors in strict mode; this is defensive.
        }
        Ok(EditOutcome {
            report,
            document: Some(Docx { pkg, path: None }),
        })
    }
}

/// Result of an edit batch.
pub struct EditOutcome {
    /// Per-op report.
    pub report: EditReport,
    /// Edited document (absent on dry-run).
    pub document: Option<Docx>,
}

/// Builder for a [`TrackOp`] batch.
pub struct TrackBuilder<'a> {
    doc: &'a Docx,
    ops: Vec<TrackOp>,
    options: TrackOptions,
}

impl TrackBuilder<'_> {
    /// Default author for ops that do not name one.
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.options.author = Some(author.into());
        self
    }

    /// Default ISO date for ops that do not name one.
    pub fn date(mut self, date: impl Into<String>) -> Self {
        self.options.date = Some(date.into());
        self
    }

    /// Redline granularity (minimized after LCS).
    pub fn granularity(mut self, g: Granularity) -> Self {
        self.options.granularity = g;
        self
    }

    /// Best-effort mode.
    pub fn lenient(mut self) -> Self {
        self.options.lenient = true;
        self
    }

    /// Validate without writing.
    pub fn dry_run(mut self) -> Self {
        self.options.dry_run = true;
        self
    }

    /// Apply this batch to a story part.
    pub fn part(mut self, part: impl Into<String>) -> Self {
        self.options.part = Some(part.into());
        self
    }

    /// Apply the batch.
    pub fn apply(self) -> Result<TrackOutcome, DocxError> {
        let mut pkg = self.doc.pkg.clone();
        let report = track::apply(&mut pkg, &self.ops, &self.options)?;
        if self.options.dry_run {
            return Ok(TrackOutcome {
                report,
                document: None,
            });
        }
        Ok(TrackOutcome {
            report,
            document: Some(Docx { pkg, path: None }),
        })
    }
}

/// Result of a track batch.
pub struct TrackOutcome {
    /// Per-op report.
    pub report: TrackReport,
    /// Edited document (absent on dry-run).
    pub document: Option<Docx>,
}

/// Compare two documents into a redlined third.
pub fn redline(
    original: &Docx,
    revised: &Docx,
    author: &str,
    granularity: Granularity,
) -> Result<Docx, DocxError> {
    let pkg = crate::redline::redline(&original.pkg, &revised.pkg, author, granularity)?;
    Ok(Docx { pkg, path: None })
}

/// Compare two documents into a redlined third using [`diff_minimal`](blackline_core::diff::diff_minimal).
pub fn track_redline(
    original: &Docx,
    revised: &Docx,
    author: &str,
    granularity: Granularity,
) -> Result<Docx, DocxError> {
    let pkg = crate::redline::redline_minimal(&original.pkg, &revised.pkg, author, granularity)?;
    Ok(Docx { pkg, path: None })
}
