//! Bounded, scoped search over the numbered view.

use serde::Serialize;

use crate::error::DocxError;
use crate::text::{view_lines, ViewLine};
use blackline_core::textutil::{find_normalized_ci, is_whole_word, normalize_quotes};
use blackline_core::xml::XmlDocument;

/// Default match cap.
pub const DEFAULT_LIMIT: usize = 50;

/// A search query.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// Literal needle.
    pub needle: String,
    /// Case-sensitive match.
    pub case_sensitive: bool,
    /// Whole-word only.
    pub whole_word: bool,
    /// Maximum matches (0 = unlimited).
    pub limit: usize,
    /// Inclusive view-index lower bound.
    pub from: Option<usize>,
    /// Inclusive view-index upper bound.
    pub to: Option<usize>,
}

impl SearchQuery {
    /// Literal search with the default limit.
    pub fn new(needle: impl Into<String>) -> Self {
        Self {
            needle: needle.into(),
            case_sensitive: false,
            whole_word: false,
            limit: DEFAULT_LIMIT,
            from: None,
            to: None,
        }
    }

    /// Restrict to whole words.
    pub fn whole_word(mut self) -> Self {
        self.whole_word = true;
        self
    }

    /// Set the match cap.
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n;
        self
    }
}

/// One match.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    /// View index.
    pub index: usize,
    /// Full line text.
    pub text: String,
    /// Byte offset of the match within `text`.
    pub offset: usize,
}

/// Search results.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    /// Hits (capped by the query limit).
    pub matches: Vec<SearchHit>,
    /// Total hits found before the limit cut off. Equal to `matches.len()`
    /// when the scan completed.
    pub total: usize,
    /// True when more matches exist beyond `limit`.
    pub truncated: bool,
}

/// Run `query` over the document view.
pub fn search(doc: &XmlDocument, query: &SearchQuery) -> Result<SearchResults, DocxError> {
    let lines = view_lines(doc, false)?;
    search_lines(&lines, query)
}

/// Search already-rendered lines (used by tests).
pub fn search_lines(lines: &[ViewLine], query: &SearchQuery) -> Result<SearchResults, DocxError> {
    let needle = normalize_quotes(&query.needle);
    let mut matches = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;

    for line in lines {
        if let Some(from) = query.from {
            if line.index < from {
                continue;
            }
        }
        if let Some(to) = query.to {
            if line.index > to {
                continue;
            }
        }
        let found = if query.case_sensitive {
            blackline_core::textutil::find_normalized(&line.text, &needle)
        } else {
            find_normalized_ci(&line.text, &needle)
        };
        if let Some((offset, matched)) = found {
            if query.whole_word && !is_whole_word(&line.text, offset, matched.len()) {
                continue;
            }
            total += 1;
            if query.limit > 0 && matches.len() >= query.limit {
                truncated = true;
                break;
            }
            matches.push(SearchHit {
                index: line.index,
                text: line.text.clone(),
                offset,
            });
        }
    }
    if !truncated {
        total = matches.len();
    }
    Ok(SearchResults {
        matches,
        total,
        truncated,
    })
}
