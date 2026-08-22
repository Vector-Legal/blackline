//! Apply a batch of [`EditOp`]s to an open document.

use serde::Serialize;

use blackline_core::diff::Granularity;
use blackline_core::package::Package;
use blackline_core::xml::{XmlDocument, XmlNode};

use crate::body::{
    find_body_mut, get_mut_path, resolve_match, resolve_view_index, visible_text, BodyKind,
};
use crate::comment::{self, insert_comment};
use crate::error::DocxError;
use crate::hyperlink;
use crate::ops::EditOp;
use crate::revision::{accept_change, next_change_id, reject_change, settle_all};
use crate::splice;
use crate::style::{build_paragraph, build_run, ParaProps, RunProps};

/// Options for an edit batch.
#[derive(Debug, Clone, Default)]
pub struct EditOptions {
    /// Author for tracked changes and comments.
    pub author: Option<String>,
    /// Emit `w:ins` / `w:del` instead of mutating visible text.
    pub tracked: bool,
    /// Redline granularity.
    pub granularity: Granularity,
    /// Apply what can be applied.
    pub lenient: bool,
    /// Resolve without writing.
    pub dry_run: bool,
    /// Story part to edit. `None` is the main document. `"header"` / `"footer"`
    /// select the first header/footer that has text; a path like
    /// `word/header2.xml` selects that part.
    pub part: Option<String>,
}

/// Per-op status.
#[derive(Debug, Clone, Serialize)]
pub struct OpReport {
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
pub struct EditReport {
    /// Applied count.
    pub applied: usize,
    /// Failed count.
    pub failed: usize,
    /// `strict`, `lenient`, or `dry-run`.
    pub mode: &'static str,
    /// Per-op rows.
    pub ops: Vec<OpReport>,
}

/// Apply `ops` to `pkg`. Mutates the package unless `dry_run`.
pub fn apply(
    pkg: &mut Package,
    ops: &[EditOp],
    opts: &EditOptions,
) -> Result<EditReport, DocxError> {
    if opts.tracked && opts.author.is_none() {
        return Err(DocxError::AuthorRequired);
    }
    if ops.iter().any(|o| o.needs_author()) && opts.author.is_none() {
        // insert_comment may carry its own author
        let missing = ops.iter().any(|o| match o {
            EditOp::InsertComment { author, .. } => author.is_none() && opts.author.is_none(),
            _ => false,
        });
        if missing {
            return Err(DocxError::AuthorRequired);
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
        match apply_one(pkg, &part, &mut doc, op, opts, &mut next_id) {
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
    Ok(EditReport {
        applied,
        failed,
        mode,
        ops: reports,
    })
}

fn ok(i: usize, op: &EditOp, detail: String) -> OpReport {
    OpReport {
        index: i,
        op: op.name().into(),
        status: "applied",
        detail,
    }
}

fn fail(i: usize, op: &EditOp, detail: String) -> OpReport {
    OpReport {
        index: i,
        op: op.name().into(),
        status: "failed",
        detail,
    }
}

fn preview(doc: &XmlDocument, op: &EditOp) -> Result<String, DocxError> {
    match op {
        EditOp::Replace {
            index,
            content_match,
            old,
            ..
        } => {
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let para = body_child(doc, idx)?;
            if visible_text(para).contains(&blackline_core::textutil::normalize_quotes(old))
                || blackline_core::textutil::find_normalized(&visible_text(para), old).is_some()
            {
                Ok(format!("would replace in [{idx}]"))
            } else {
                Err(DocxError::invalid(format!(
                    "text {old:?} not found in [{idx}]"
                )))
            }
        }
        EditOp::Insert {
            index,
            content_match,
            ..
        }
        | EditOp::Delete {
            index,
            content_match,
            ..
        }
        | EditOp::DeleteRun {
            index,
            content_match,
            ..
        } => {
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            Ok(format!("would apply to [{idx}]"))
        }
        EditOp::Format { index, .. }
        | EditOp::TableInsertRow { index, .. }
        | EditOp::TableDeleteRow { index, .. } => {
            let _ = resolve_view_index(doc, *index)?;
            Ok(format!("would apply to [{index}]"))
        }
        _ => Ok("would apply".into()),
    }
}

fn apply_one(
    pkg: &mut Package,
    part: &str,
    doc: &mut XmlDocument,
    op: &EditOp,
    opts: &EditOptions,
    next_id: &mut usize,
) -> Result<String, DocxError> {
    match op {
        EditOp::Replace {
            index,
            old,
            new,
            content_match,
        } => {
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let para = body_child_mut(doc, idx)?;
            let ok = if opts.tracked {
                let author = opts.author.as_deref().unwrap();
                splice::tracked_replace(para, old, new, author, opts.granularity, next_id)?
            } else {
                splice::plain_replace(para, old, new)?
            };
            if !ok {
                return Err(DocxError::invalid(format!(
                    "text {old:?} not found in [{idx}]"
                )));
            }
            Ok(format!("replaced in [{idx}]"))
        }
        EditOp::Insert {
            index,
            position,
            content_match,
            text,
            content,
            style,
            para_props,
        } => {
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let body_i = resolve_view_index(doc, idx)?;
            let insert_at = if position == "before" {
                body_i
            } else {
                body_i + 1
            };
            let body_text = text.clone().or_else(|| content.clone()).unwrap_or_default();
            let mut para = build_paragraph(&body_text, style.as_deref());
            para_props.apply_to_para(&mut para);
            if opts.tracked {
                let author = opts.author.as_deref().unwrap();
                // Track the whole paragraph as an insertion.
                let id = *next_id;
                *next_id += 1;
                let date = blackline_core::time::utc_now_iso();
                let run = para
                    .find_child("r")
                    .cloned()
                    .unwrap_or_else(|| build_run(&body_text, None));
                let ins = XmlNode::w("ins")
                    .with_attr("w:id", id.to_string())
                    .with_attr("w:author", author)
                    .with_attr("w:date", date)
                    .with_child(run);
                // rebuild para children: pPr + ins
                let ppr = para.find_child("pPr").cloned();
                let mut kids = Vec::new();
                if let Some(p) = ppr {
                    kids.push(p);
                }
                kids.push(ins);
                *para.children_mut() = kids;
            }
            let body = find_body_mut(&mut doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
            let kids = body.children_mut();
            if insert_at > kids.len() {
                return Err(DocxError::invalid("insert index out of range"));
            }
            kids.insert(insert_at, para);
            Ok(format!("inserted at body[{insert_at}]"))
        }
        EditOp::Delete {
            index,
            range,
            content_match,
        } => {
            let (from, to) = if let Some((a, b)) = range {
                (*a, *b)
            } else {
                let idx = resolve_target(doc, *index, content_match.as_deref())?;
                (idx, idx)
            };
            if from == 0 || to < from {
                return Err(DocxError::invalid("invalid delete range"));
            }
            // Delete from high to low so indices stay stable.
            for view in (from..=to).rev() {
                let body_i = resolve_view_index(doc, view)?;
                let body =
                    find_body_mut(&mut doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
                body.children_mut().remove(body_i);
            }
            Ok(format!("deleted [{from}..{to}]"))
        }
        EditOp::DeleteRun {
            index,
            content_match,
            text,
        } => {
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let para = body_child_mut(doc, idx)?;
            let removed = remove_run_by_text(para, text);
            if !removed {
                return Err(DocxError::invalid(format!("run {text:?} not found")));
            }
            Ok(format!("deleted run in [{idx}]"))
        }
        EditOp::Format {
            index,
            run_props,
            para_props,
        } => {
            let para = body_child_mut(doc, *index)?;
            para_props.apply_to_para(para);
            apply_run_props(para, run_props);
            Ok(format!("formatted [{index}]"))
        }
        EditOp::TableInsertRow {
            index,
            row_index,
            position,
            cells,
        } => {
            let tbl = body_child_mut(doc, *index)?;
            if !tbl.is_element_with_local_name("tbl") {
                return Err(DocxError::invalid("target is not a table"));
            }
            insert_table_row(tbl, *row_index, position, cells)?;
            Ok(format!("inserted row in table [{index}]"))
        }
        EditOp::TableDeleteRow { index, row_index } => {
            let tbl = body_child_mut(doc, *index)?;
            delete_table_row(tbl, *row_index)?;
            Ok(format!("deleted row {row_index} in table [{index}]"))
        }
        EditOp::InsertComment {
            index,
            content_match,
            anchor,
            text,
            author,
        } => {
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let author = author
                .as_deref()
                .or(opts.author.as_deref())
                .ok_or(DocxError::AuthorRequired)?;
            let para = body_child_mut(doc, idx)?;
            let needle = anchor.clone().unwrap_or_else(|| visible_text(para));
            let id = insert_comment(pkg, para, &needle, text, author)?;
            Ok(format!("comment {id} on [{idx}]"))
        }
        EditOp::DeleteComment { id } => {
            let removed = comment::delete_comment(pkg, &mut doc.root, id)?;
            if !removed {
                return Err(DocxError::invalid(format!("comment {id} not found")));
            }
            Ok(format!("deleted comment {id}"))
        }
        EditOp::AcceptChange { id } => {
            if !accept_change(&mut doc.root, id) {
                return Err(DocxError::invalid(format!("change {id} not found")));
            }
            Ok(format!("accepted {id}"))
        }
        EditOp::RejectChange { id } => {
            if !reject_change(&mut doc.root, id) {
                return Err(DocxError::invalid(format!("change {id} not found")));
            }
            Ok(format!("rejected {id}"))
        }
        EditOp::AcceptAll { author } => {
            let n = settle_all(&mut doc.root, true, author.as_deref());
            Ok(format!("accepted {n} change(s)"))
        }
        EditOp::RejectAll { author } => {
            let n = settle_all(&mut doc.root, false, author.as_deref());
            Ok(format!("rejected {n} change(s)"))
        }
        EditOp::SetHyperlink {
            index,
            content_match,
            text,
            url,
        } => {
            let idx = resolve_target(doc, *index, content_match.as_deref())?;
            let para = body_child_mut(doc, idx)?;
            hyperlink::set_hyperlink(pkg, part, para, text, url)
        }
    }
}

fn remove_run_by_text(node: &mut XmlNode, text: &str) -> bool {
    let Some(kids) = node.try_children_mut() else {
        return false;
    };
    let before = kids.len();
    kids.retain(|c| !(c.is_element_with_local_name("r") && visible_text(c) == text));
    if kids.len() < before {
        return true;
    }
    for kid in kids.iter_mut() {
        if remove_run_by_text(kid, text) {
            return true;
        }
    }
    false
}

/// Resolve `"header"` / `"footer"` / a part name / `None` (main document).
pub fn resolve_part(pkg: &Package, spec: Option<&str>) -> Result<String, DocxError> {
    match spec {
        None => Ok(pkg.main_document_part()?),
        Some("header") => first_story_part(pkg, "header")
            .ok_or_else(|| DocxError::invalid("no header part with text")),
        Some("footer") => first_story_part(pkg, "footer")
            .ok_or_else(|| DocxError::invalid("no footer part with text")),
        Some(name) => {
            if pkg.has_part(name) {
                Ok(name.to_string())
            } else {
                Err(DocxError::invalid(format!("package has no part {name}")))
            }
        }
    }
}

/// Header and footer part names, document order.
pub fn story_parts(pkg: &Package) -> Vec<String> {
    pkg.part_names()
        .into_iter()
        .filter(|n| {
            let n = n.to_ascii_lowercase();
            n.contains("header") || n.contains("footer")
        })
        .map(str::to_string)
        .collect()
}

fn first_story_part(pkg: &Package, kind: &str) -> Option<String> {
    let mut named: Vec<String> = pkg
        .part_names()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().contains(kind))
        .map(str::to_string)
        .collect();
    named.sort();
    for name in &named {
        if let Ok(doc) = pkg.part_xml(name) {
            if !visible_text(&doc.root).trim().is_empty() {
                return Some(name.clone());
            }
        }
    }
    named.into_iter().next()
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
    resolve_view_index(doc, idx)?; // validate
    Ok(idx)
}

fn body_child(doc: &XmlDocument, view_index: usize) -> Result<&XmlNode, DocxError> {
    let i = resolve_view_index(doc, view_index)?;
    let body = crate::body::find_body(&doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
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

fn apply_run_props(para: &mut XmlNode, props: &RunProps) {
    if props.is_empty() {
        return;
    }
    if let Some(kids) = para.try_children_mut() {
        for kid in kids.iter_mut() {
            apply_run_props_to(kid, props);
        }
    }
}

fn apply_run_props_to(node: &mut XmlNode, props: &RunProps) {
    if node.is_element_with_local_name("r") {
        if let Some(rpr) = node.find_child_mut("rPr") {
            props.apply_to_rpr(rpr);
        } else if let Some(rpr) = props.to_rpr() {
            node.children_mut().insert(0, rpr);
        }
        return;
    }
    if let Some(kids) = node.try_children_mut() {
        for kid in kids.iter_mut() {
            apply_run_props_to(kid, props);
        }
    }
}

fn insert_table_row(
    tbl: &mut XmlNode,
    row_index: usize,
    position: &str,
    cells: &[String],
) -> Result<(), DocxError> {
    if row_index == 0 {
        return Err(DocxError::invalid("row_index is 1-based"));
    }
    let mut row_positions = Vec::new();
    for (i, child) in tbl.children().iter().enumerate() {
        if child.is_element_with_local_name("tr") {
            row_positions.push(i);
        }
    }
    if row_positions.is_empty() {
        return Err(DocxError::invalid("table has no rows"));
    }
    let target = row_positions
        .get(row_index - 1)
        .copied()
        .ok_or_else(|| DocxError::invalid("row_index out of range"))?;
    let insert_at = if position == "before" {
        target
    } else {
        target + 1
    };
    let mut tr = XmlNode::w("tr");
    for cell in cells {
        let tc = XmlNode::w("tc").with_child(build_paragraph(cell, None));
        tr = tr.with_child(tc);
    }
    tbl.children_mut().insert(insert_at, tr);
    Ok(())
}

fn delete_table_row(tbl: &mut XmlNode, row_index: usize) -> Result<(), DocxError> {
    if row_index == 0 {
        return Err(DocxError::invalid("row_index is 1-based"));
    }
    let mut nth = 0;
    let mut pos = None;
    for (i, child) in tbl.children().iter().enumerate() {
        if child.is_element_with_local_name("tr") {
            nth += 1;
            if nth == row_index {
                pos = Some(i);
                break;
            }
        }
    }
    let pos = pos.ok_or_else(|| DocxError::invalid("row_index out of range"))?;
    tbl.children_mut().remove(pos);
    Ok(())
}

#[allow(dead_code)]
fn _unused(_: BodyKind, _: ParaProps, _: Option<&mut XmlNode>) {
    let _ = get_mut_path;
}
