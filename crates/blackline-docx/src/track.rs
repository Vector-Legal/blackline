//! Track module: multi-author redline recipe on top of surgical `w:ins` /
//! `w:del`. Opinionated relative to `docx edit --track` — each op can carry
//! its own author and date, deletes wrap only the matched span, and
//! replacements go through [`blackline_core::diff::diff_minimal`].

use serde::{Deserialize, Serialize};

use blackline_core::diff::Granularity;
use blackline_core::package::Package;
use blackline_core::textutil::{find_normalized, normalize_quotes};
use blackline_core::xml::{XmlDocument, XmlNode};

use crate::body::{find_body, find_body_mut, resolve_match, resolve_view_index, visible_text};
use crate::comment::insert_comment_dated;
use crate::edit::resolve_part;
use crate::error::DocxError;
use crate::revision::{accept_change, next_change_id, reject_change, settle_all};
use crate::splice::{apply_diff_surgically_minimal, surgical_delete, tracked_insert_at};
use crate::style::{build_paragraph, build_run};

/// Options for a [`TrackOp`] batch.
#[derive(Debug, Clone, Default)]
pub struct TrackOptions {
    /// Default author when an op does not name one.
    pub author: Option<String>,
    /// Default `w:date` when an op does not name one.
    pub date: Option<String>,
    /// Redline granularity (default word). Minimized after LCS.
    pub granularity: Granularity,
    /// Apply what can be applied.
    pub lenient: bool,
    /// Resolve without writing.
    pub dry_run: bool,
    /// Story part. `None` is the main document.
    pub part: Option<String>,
}

/// Per-op status.
#[derive(Debug, Clone, Serialize)]
pub struct TrackOpReport {
    /// Zero-based batch index.
    pub index: usize,
    /// Op name.
    pub op: String,
    /// `applied` or `failed`.
    pub status: &'static str,
    /// Detail.
    pub detail: String,
}

/// Batch report.
#[derive(Debug, Clone, Serialize)]
pub struct TrackReport {
    /// Applied count.
    pub applied: usize,
    /// Failed count.
    pub failed: usize,
    /// `strict`, `lenient`, or `dry-run`.
    pub mode: &'static str,
    /// Per-op rows.
    pub ops: Vec<TrackOpReport>,
}

/// One track / redline operation. Each mutating op may carry its own
/// `author` and optional ISO `date`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum TrackOp {
    /// Surgical replace: minimize the delete+insert so shared text stays.
    #[serde(rename = "replace")]
    Replace {
        /// 1-based view index.
        #[serde(default)]
        index: Option<usize>,
        /// Locate the paragraph by content.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
        /// Text to replace. Defaults to `match` when omitted.
        #[serde(default)]
        old: Option<String>,
        /// Replacement text.
        new: String,
        /// Author for this change.
        #[serde(default)]
        author: Option<String>,
        /// ISO date for this change.
        #[serde(default)]
        date: Option<String>,
    },
    /// Insert tracked text at a position relative to a match (or the
    /// paragraph start / end).
    #[serde(rename = "insert", alias = "add")]
    Insert {
        /// 1-based view index.
        #[serde(default)]
        index: Option<usize>,
        /// Locate the paragraph / span.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
        /// `before`, `after`, `start`, or `end`.
        #[serde(default = "default_after")]
        position: String,
        /// Text to insert.
        text: String,
        /// Author for this change.
        #[serde(default)]
        author: Option<String>,
        /// ISO date for this change.
        #[serde(default)]
        date: Option<String>,
    },
    /// Insert a whole paragraph as a tracked insertion.
    #[serde(rename = "insert_paragraph")]
    InsertParagraph {
        /// Anchor view index.
        #[serde(default)]
        index: Option<usize>,
        /// Content-match anchor.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
        /// `before` or `after` the anchor paragraph.
        #[serde(default = "default_after")]
        position: String,
        /// Paragraph text.
        text: String,
        /// Author for this change.
        #[serde(default)]
        author: Option<String>,
        /// ISO date for this change.
        #[serde(default)]
        date: Option<String>,
    },
    /// Surgical delete: wrap only the matched span in `w:del`. When
    /// `text` / `old` is omitted, the whole visible paragraph is marked
    /// deleted (the node stays).
    #[serde(rename = "delete")]
    Delete {
        /// 1-based view index.
        #[serde(default)]
        index: Option<usize>,
        /// Locate the paragraph by content.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
        /// Span to delete.
        #[serde(default)]
        text: Option<String>,
        /// Alias for `text`.
        #[serde(default)]
        old: Option<String>,
        /// Author for this change.
        #[serde(default)]
        author: Option<String>,
        /// ISO date for this change.
        #[serde(default)]
        date: Option<String>,
    },
    /// Insert a comment with a custom author and optional date.
    #[serde(rename = "comment", alias = "insert_comment")]
    Comment {
        /// 1-based view index.
        #[serde(default)]
        index: Option<usize>,
        /// Locate the paragraph by content.
        #[serde(default, rename = "match")]
        content_match: Option<String>,
        /// Text the comment range wraps.
        #[serde(default)]
        anchor: Option<String>,
        /// Comment body.
        text: String,
        /// Author for this comment.
        #[serde(default)]
        author: Option<String>,
        /// ISO date for this comment.
        #[serde(default)]
        date: Option<String>,
    },
    /// Accept one tracked change by `w:id`.
    #[serde(rename = "accept")]
    Accept {
        /// `w:id`.
        id: String,
    },
    /// Reject one tracked change by `w:id`.
    #[serde(rename = "reject")]
    Reject {
        /// `w:id`.
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
}

fn default_after() -> String {
    "after".into()
}

impl TrackOp {
    /// Stable op name.
    pub fn name(&self) -> &'static str {
        match self {
            TrackOp::Replace { .. } => "replace",
            TrackOp::Insert { .. } => "insert",
            TrackOp::InsertParagraph { .. } => "insert_paragraph",
            TrackOp::Delete { .. } => "delete",
            TrackOp::Comment { .. } => "comment",
            TrackOp::Accept { .. } => "accept",
            TrackOp::Reject { .. } => "reject",
            TrackOp::AcceptAll { .. } => "accept_all",
            TrackOp::RejectAll { .. } => "reject_all",
        }
    }

    /// True when this op already names a non-empty author.
    pub fn has_own_author(&self) -> bool {
        self.op_author()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_some()
    }

    /// True when this op writes a revision or comment and needs an author.
    pub fn needs_author(&self) -> bool {
        matches!(
            self,
            TrackOp::Replace { .. }
                | TrackOp::Insert { .. }
                | TrackOp::InsertParagraph { .. }
                | TrackOp::Delete { .. }
                | TrackOp::Comment { .. }
        )
    }

    fn op_author(&self) -> Option<&str> {
        match self {
            TrackOp::Replace { author, .. }
            | TrackOp::Insert { author, .. }
            | TrackOp::InsertParagraph { author, .. }
            | TrackOp::Delete { author, .. }
            | TrackOp::Comment { author, .. } => author.as_deref(),
            _ => None,
        }
    }

    fn op_date(&self) -> Option<&str> {
        match self {
            TrackOp::Replace { date, .. }
            | TrackOp::Insert { date, .. }
            | TrackOp::InsertParagraph { date, .. }
            | TrackOp::Delete { date, .. }
            | TrackOp::Comment { date, .. } => date.as_deref(),
            _ => None,
        }
    }
}

/// Apply `ops` to `pkg`. Mutates the package unless `dry_run`.
pub fn apply(
    pkg: &mut Package,
    ops: &[TrackOp],
    opts: &TrackOptions,
) -> Result<TrackReport, DocxError> {
    for op in ops {
        if op.needs_author() {
            resolve_author(op.op_author(), opts)?;
        }
    }

    let part = resolve_part(pkg, opts.part.as_deref())?;
    let mut doc = pkg.part_xml(&part)?;
    let mut reports = Vec::new();
    let mut next_id = next_change_id(&doc.root);

    for (i, op) in ops.iter().enumerate() {
        if opts.dry_run {
            match preview(&doc, op) {
                Ok(detail) => reports.push(ok(i, op, detail)),
                Err(e) => {
                    reports.push(fail(i, op, e.to_string()));
                    if !opts.lenient {
                        break;
                    }
                }
            }
            continue;
        }
        match apply_one(pkg, &mut doc, op, opts, &mut next_id) {
            Ok(detail) => reports.push(ok(i, op, detail)),
            Err(e) => {
                reports.push(fail(i, op, e.to_string()));
                if !opts.lenient {
                    return Err(DocxError::OpFailed {
                        index: i,
                        op: op.name().into(),
                        reason: e.to_string(),
                    });
                }
            }
        }
    }

    if !opts.dry_run {
        pkg.set_part_xml(&part, &doc);
    }

    let applied = reports.iter().filter(|r| r.status == "applied").count();
    let failed = reports.iter().filter(|r| r.status == "failed").count();
    let mode = if opts.dry_run {
        "dry-run"
    } else if opts.lenient {
        "lenient"
    } else {
        "strict"
    };
    Ok(TrackReport {
        applied,
        failed,
        mode,
        ops: reports,
    })
}

fn ok(i: usize, op: &TrackOp, detail: String) -> TrackOpReport {
    TrackOpReport {
        index: i,
        op: op.name().into(),
        status: "applied",
        detail,
    }
}

fn fail(i: usize, op: &TrackOp, detail: String) -> TrackOpReport {
    TrackOpReport {
        index: i,
        op: op.name().into(),
        status: "failed",
        detail,
    }
}

fn resolve_author(op_author: Option<&str>, opts: &TrackOptions) -> Result<String, DocxError> {
    if let Some(a) = op_author.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(a.to_string());
    }
    if let Some(a) = opts
        .author
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(a.to_string());
    }
    Err(DocxError::AuthorRequired)
}

fn resolve_date(op_date: Option<&str>, opts: &TrackOptions) -> Option<String> {
    op_date
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            opts.date
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
}

fn stamp_new_revisions(root: &mut XmlNode, from_id: usize, until_id: usize, date: &str) {
    root.walk_mut(&mut |n| {
        if n.is_element_with_local_name("ins") || n.is_element_with_local_name("del") {
            if let Some(id) = n.get_attr("id") {
                if let Ok(id) = id.parse::<usize>() {
                    if id >= from_id && id < until_id {
                        n.set_attr("w:date", date);
                    }
                }
            }
        }
    });
}

fn resolve_target(
    doc: &XmlDocument,
    index: Option<usize>,
    content_match: Option<&str>,
) -> Result<usize, DocxError> {
    if let Some(m) = content_match {
        return resolve_match(doc, m);
    }
    let idx = index.ok_or_else(|| DocxError::invalid("op needs index or match"))?;
    resolve_view_index(doc, idx)?;
    Ok(idx)
}

fn body_child(doc: &XmlDocument, view_index: usize) -> Result<&XmlNode, DocxError> {
    let i = resolve_view_index(doc, view_index)?;
    let body = find_body(&doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
    body.children()
        .get(i)
        .ok_or_else(|| DocxError::invalid("body child missing"))
}

fn body_child_mut(doc: &mut XmlDocument, view_index: usize) -> Result<&mut XmlNode, DocxError> {
    let i = resolve_view_index(doc, view_index)?;
    let body = find_body_mut(&mut doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
    body.children_mut()
        .get_mut(i)
        .ok_or_else(|| DocxError::invalid("body child missing"))
}

fn preview(doc: &XmlDocument, op: &TrackOp) -> Result<String, DocxError> {
    match op {
        TrackOp::Replace {
            index,
            content_match,
            old,
            ..
        } => {
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let needle = old
                .as_deref()
                .or(content_match.as_deref())
                .ok_or_else(|| DocxError::invalid("replace needs old or match"))?;
            let para = body_child(doc, idx)?;
            if find_normalized(&visible_text(para), needle).is_some() {
                Ok(format!("would replace in [{idx}]"))
            } else {
                Err(DocxError::invalid(format!(
                    "text {needle:?} not found in [{idx}]"
                )))
            }
        }
        TrackOp::Insert {
            index,
            content_match,
            ..
        }
        | TrackOp::InsertParagraph {
            index,
            content_match,
            ..
        }
        | TrackOp::Delete {
            index,
            content_match,
            ..
        }
        | TrackOp::Comment {
            index,
            content_match,
            ..
        } => {
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            Ok(format!("would apply to [{idx}]"))
        }
        TrackOp::Accept { id } => Ok(format!("would accept {id}")),
        TrackOp::Reject { id } => Ok(format!("would reject {id}")),
        TrackOp::AcceptAll { .. } => Ok("would accept_all".into()),
        TrackOp::RejectAll { .. } => Ok("would reject_all".into()),
    }
}

fn apply_one(
    pkg: &mut Package,
    doc: &mut XmlDocument,
    op: &TrackOp,
    opts: &TrackOptions,
    next_id: &mut usize,
) -> Result<String, DocxError> {
    let start_id = *next_id;
    let date = resolve_date(op.op_date(), opts);
    let detail = match op {
        TrackOp::Replace {
            index,
            content_match,
            old,
            new,
            ..
        } => {
            let author = resolve_author(op.op_author(), opts)?;
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let needle = old
                .as_deref()
                .or(content_match.as_deref())
                .ok_or_else(|| DocxError::invalid("replace needs old or match"))?;
            let para = body_child_mut(doc, idx)?;
            apply_replace(para, needle, new, &author, opts.granularity, next_id)?;
            format!("replaced in [{idx}]")
        }
        TrackOp::Insert {
            index,
            content_match,
            position,
            text,
            ..
        } => {
            let author = resolve_author(op.op_author(), opts)?;
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let para = body_child_mut(doc, idx)?;
            apply_insert(
                para,
                content_match.as_deref(),
                position,
                text,
                &author,
                next_id,
            )?;
            format!("inserted in [{idx}]")
        }
        TrackOp::InsertParagraph {
            index,
            content_match,
            position,
            text,
            ..
        } => {
            let author = resolve_author(op.op_author(), opts)?;
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            apply_insert_paragraph(doc, idx, position, text, &author, date.as_deref(), next_id)?;
            format!("inserted paragraph around [{idx}]")
        }
        TrackOp::Delete {
            index,
            content_match,
            text,
            old,
            ..
        } => {
            let author = resolve_author(op.op_author(), opts)?;
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let para = body_child_mut(doc, idx)?;
            let span = text
                .as_deref()
                .or(old.as_deref())
                .map(str::to_string)
                .unwrap_or_else(|| visible_text(para));
            if span.is_empty() {
                return Err(DocxError::invalid("delete span is empty"));
            }
            if !surgical_delete(para, &span, &author, next_id)? {
                return Err(DocxError::invalid(format!(
                    "text {span:?} not found in [{idx}]"
                )));
            }
            format!("deleted in [{idx}]")
        }
        TrackOp::Comment {
            index,
            content_match,
            anchor,
            text,
            ..
        } => {
            let author = resolve_author(op.op_author(), opts)?;
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let para = body_child_mut(doc, idx)?;
            let needle = anchor
                .clone()
                .or_else(|| content_match.clone())
                .unwrap_or_else(|| visible_text(para));
            let id = insert_comment_dated(pkg, para, &needle, text, &author, date.as_deref())?;
            format!("comment {id} on [{idx}]")
        }
        TrackOp::Accept { id } => {
            if !accept_change(&mut doc.root, id) {
                return Err(DocxError::invalid(format!("change {id} not found")));
            }
            format!("accepted {id}")
        }
        TrackOp::Reject { id } => {
            if !reject_change(&mut doc.root, id) {
                return Err(DocxError::invalid(format!("change {id} not found")));
            }
            format!("rejected {id}")
        }
        TrackOp::AcceptAll { author } => {
            let n = settle_all(&mut doc.root, true, author.as_deref());
            format!("accepted {n} change(s)")
        }
        TrackOp::RejectAll { author } => {
            let n = settle_all(&mut doc.root, false, author.as_deref());
            format!("rejected {n} change(s)")
        }
    };
    if let Some(date) = date {
        if *next_id > start_id {
            stamp_new_revisions(&mut doc.root, start_id, *next_id, &date);
        }
    }
    Ok(detail)
}

fn apply_replace(
    para: &mut XmlNode,
    old: &str,
    new: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
) -> Result<(), DocxError> {
    let text = visible_text(para);
    let Some((start, matched)) = find_normalized(&text, old) else {
        return Err(DocxError::invalid(format!("text {old:?} not found")));
    };
    if normalize_quotes(&matched) == normalize_quotes(new) {
        return Ok(());
    }
    let revised = format!(
        "{}{}{}",
        &text[..start],
        new,
        &text[start + matched.len()..]
    );
    apply_diff_surgically_minimal(para, &text, &revised, author, granularity, next_id)
}

fn apply_insert(
    para: &mut XmlNode,
    content_match: Option<&str>,
    position: &str,
    text: &str,
    author: &str,
    next_id: &mut usize,
) -> Result<(), DocxError> {
    if text.is_empty() {
        return Ok(());
    }
    let visible = visible_text(para);
    let pos = match position {
        "start" => 0,
        "end" => visible.len(),
        "before" | "after" => {
            let needle = content_match
                .ok_or_else(|| DocxError::invalid("insert before/after needs match"))?;
            let (start, matched) = find_normalized(&visible, needle)
                .ok_or_else(|| DocxError::invalid(format!("insert anchor {needle:?} not found")))?;
            if position == "before" {
                start
            } else {
                start + matched.len()
            }
        }
        other => {
            return Err(DocxError::invalid(format!(
                "unknown insert position '{other}' (expected before, after, start, or end)"
            )));
        }
    };
    if !tracked_insert_at(para, pos, text, author, next_id)? {
        return Err(DocxError::invalid(
            "could not insert at the requested offset",
        ));
    }
    Ok(())
}

fn apply_insert_paragraph(
    doc: &mut XmlDocument,
    idx: usize,
    position: &str,
    text: &str,
    author: &str,
    date: Option<&str>,
    next_id: &mut usize,
) -> Result<(), DocxError> {
    let body_i = resolve_view_index(doc, idx)?;
    let insert_at = match position {
        "before" => body_i,
        "after" => body_i + 1,
        other => {
            return Err(DocxError::invalid(format!(
                "unknown insert_paragraph position '{other}' (expected before or after)"
            )));
        }
    };
    let mut para = build_paragraph(text, None);
    let id = *next_id;
    *next_id += 1;
    let stamp = date
        .map(str::to_string)
        .unwrap_or_else(blackline_core::time::utc_now_iso);
    let run = para
        .find_child("r")
        .cloned()
        .unwrap_or_else(|| build_run(text, None));
    let ins = XmlNode::w("ins")
        .with_attr("w:id", id.to_string())
        .with_attr("w:author", author)
        .with_attr("w:date", stamp)
        .with_child(run);
    let ppr = para.find_child("pPr").cloned();
    let mut kids = Vec::new();
    if let Some(p) = ppr {
        kids.push(p);
    }
    kids.push(ins);
    *para.children_mut() = kids;
    let body = find_body_mut(&mut doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
    let children = body.children_mut();
    if insert_at > children.len() {
        return Err(DocxError::invalid("insert index out of range"));
    }
    children.insert(insert_at, para);
    Ok(())
}
