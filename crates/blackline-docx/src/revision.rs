//! Tracked-change primitives: wrap text in `w:ins` / `w:del`, accept, reject.

use blackline_core::diff::{diff, diff_minimal, DiffHunk, Granularity};
use blackline_core::textutil::find_normalized;
use blackline_core::time::utc_now_iso;
use blackline_core::xml::{max_numeric_id, XmlNode};

use crate::body::{visible_text, BodyKind};
use crate::error::DocxError;
use crate::style::build_run;

/// Rebuild-based tracked replace. Used when a surgical splice cannot map
/// the match onto existing runs.
pub fn tracked_replace_rebuild(
    para: &mut XmlNode,
    old: &str,
    new: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
) -> Result<bool, DocxError> {
    let text = visible_text(para);
    let Some((start, matched)) = find_normalized(&text, old) else {
        return Ok(false);
    };
    let end = start + matched.len();
    let prefix = &text[..start];
    let suffix = &text[end..];
    let revised = format!("{prefix}{new}{suffix}");
    rebuild_with_diff(para, &text, &revised, author, granularity, next_id);
    Ok(true)
}

/// Rebuild a paragraph so the diff of `old_text` → `new_text` becomes
/// `w:del` / `w:ins` markup. Equal regions stay as ordinary runs.
pub fn rebuild_with_diff(
    para: &mut XmlNode,
    old_text: &str,
    new_text: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
) {
    rebuild_with(
        para,
        old_text,
        new_text,
        author,
        granularity,
        next_id,
        false,
    );
}

/// Like [`rebuild_with_diff`], using [`diff_minimal`].
pub fn rebuild_with_diff_minimal(
    para: &mut XmlNode,
    old_text: &str,
    new_text: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
) {
    rebuild_with(para, old_text, new_text, author, granularity, next_id, true);
}

fn rebuild_with(
    para: &mut XmlNode,
    old_text: &str,
    new_text: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
    minimal: bool,
) {
    let date = utc_now_iso();
    let rpr = first_rpr(para);
    let ppr = para.find_child("pPr").cloned();
    let hunks = if minimal {
        diff_minimal(old_text, new_text, granularity)
    } else {
        diff(old_text, new_text, granularity)
    };
    let mut children = Vec::new();
    if let Some(p) = ppr {
        children.push(p);
    }
    for hunk in hunks {
        match hunk {
            DiffHunk::Equal(s) if !s.is_empty() => {
                children.push(build_run(&s, None).with_cloned_rpr(&rpr));
            }
            DiffHunk::Delete(s) if !s.is_empty() => {
                let id = *next_id;
                *next_id += 1;
                children.push(del_wrapper(&s, author, &date, id, &rpr));
            }
            DiffHunk::Insert(s) if !s.is_empty() => {
                let id = *next_id;
                *next_id += 1;
                children.push(ins_wrapper(&s, author, &date, id, &rpr));
            }
            _ => {}
        }
    }
    if let XmlNode::Element { children: slot, .. } = para {
        *slot = children;
    }
}

fn first_rpr(para: &XmlNode) -> Option<XmlNode> {
    for r in para.find_all("r") {
        if let Some(rpr) = r.find_child("rPr") {
            return Some(rpr.clone());
        }
    }
    None
}

trait WithRpr {
    fn with_cloned_rpr(self, rpr: &Option<XmlNode>) -> Self;
}

impl WithRpr for XmlNode {
    fn with_cloned_rpr(mut self, rpr: &Option<XmlNode>) -> Self {
        if let Some(rpr) = rpr {
            if self.find_child("rPr").is_none() {
                self.children_mut().insert(0, rpr.clone());
            }
        }
        self
    }
}

/// A `w:ins` wrapping a single run of `text`.
pub(crate) fn ins_run(
    text: &str,
    author: &str,
    date: &str,
    id: usize,
    rpr: &Option<XmlNode>,
) -> XmlNode {
    ins_wrapper(text, author, date, id, rpr)
}

fn ins_wrapper(text: &str, author: &str, date: &str, id: usize, rpr: &Option<XmlNode>) -> XmlNode {
    XmlNode::w("ins")
        .with_attr("w:id", id.to_string())
        .with_attr("w:author", author)
        .with_attr("w:date", date)
        .with_child(build_run(text, None).with_cloned_rpr(rpr))
}

fn del_wrapper(text: &str, author: &str, date: &str, id: usize, rpr: &Option<XmlNode>) -> XmlNode {
    let mut t = XmlNode::w("delText").with_text(text);
    if text.starts_with(|c: char| c.is_whitespace()) || text.ends_with(|c: char| c.is_whitespace())
    {
        t.set_attr("xml:space", "preserve");
    }
    let mut run = XmlNode::w("r");
    if let Some(rpr) = rpr {
        run = run.with_child(rpr.clone());
    }
    run = run.with_child(t);
    XmlNode::w("del")
        .with_attr("w:id", id.to_string())
        .with_attr("w:author", author)
        .with_attr("w:date", date)
        .with_child(run)
}

/// Accept a tracked change: keep insertions, drop deletions.
pub fn accept_change(root: &mut XmlNode, id: &str) -> bool {
    unwrap_revision(root, id, true)
}

/// Reject a tracked change: drop insertions, keep deletions as ordinary text.
pub fn reject_change(root: &mut XmlNode, id: &str) -> bool {
    unwrap_revision(root, id, false)
}

/// Accept or reject every `w:ins` / `w:del`, optionally filtered by author.
pub fn settle_all(root: &mut XmlNode, accept: bool, author: Option<&str>) -> usize {
    let mut ids = Vec::new();
    root.walk(&mut |n| {
        if n.is_element_with_local_name("ins") || n.is_element_with_local_name("del") {
            if let Some(want) = author {
                if n.get_attr("author") != Some(want) {
                    return;
                }
            }
            if let Some(id) = n.get_attr("id") {
                ids.push(id.to_string());
            }
        }
    });
    let mut n = 0;
    for id in ids {
        if unwrap_revision(root, &id, accept) {
            n += 1;
        }
    }
    n
}

fn unwrap_revision(root: &mut XmlNode, id: &str, accept: bool) -> bool {
    // Find the parent that contains the ins/del and splice its children.
    unwrap_in(root, id, accept)
}

fn unwrap_in(node: &mut XmlNode, id: &str, accept: bool) -> bool {
    let Some(children) = node.try_children_mut() else {
        return false;
    };
    let mut i = 0;
    let mut found = false;
    while i < children.len() {
        let is_rev = {
            let c = &children[i];
            (c.is_element_with_local_name("ins") || c.is_element_with_local_name("del"))
                && c.get_attr("id") == Some(id)
        };
        if is_rev {
            let rev = children.remove(i);
            let keep = matches!((accept, rev.local_name()), (true, "ins") | (false, "del"));
            if keep {
                let inner = promote_revision_children(rev);
                for (off, child) in inner.into_iter().enumerate() {
                    children.insert(i + off, child);
                }
            }
            found = true;
            // don't increment i — we replaced this slot
            continue;
        }
        if unwrap_in(&mut children[i], id, accept) {
            found = true;
        }
        i += 1;
    }
    found
}

fn promote_revision_children(rev: XmlNode) -> Vec<XmlNode> {
    // If this is a del being kept (reject), convert delText → t.
    let is_del = rev.is_element_with_local_name("del");
    let mut out = Vec::new();
    for mut child in rev.children().to_vec() {
        if is_del {
            convert_deltext(&mut child);
        }
        out.push(child);
    }
    out
}

fn convert_deltext(node: &mut XmlNode) {
    if node.is_element_with_local_name("delText") {
        if let XmlNode::Element { local_name, .. } = node {
            *local_name = "t".into();
        }
    }
    if let Some(kids) = node.try_children_mut() {
        for kid in kids.iter_mut() {
            convert_deltext(kid);
        }
    }
}

/// Next revision id above any existing `w:id` in the tree.
pub fn next_change_id(root: &XmlNode) -> usize {
    max_numeric_id(root) + 1
}

/// List tracked changes as structured records.
pub fn list_changes(root: &XmlNode) -> Vec<TrackedChange> {
    let mut out = Vec::new();
    collect_changes(root, &mut out);
    out
}

fn collect_changes(node: &XmlNode, out: &mut Vec<TrackedChange>) {
    match node.local_name() {
        "ins" | "del" => {
            let kind = if node.local_name() == "ins" {
                "insert"
            } else {
                "delete"
            };
            let text = if kind == "insert" {
                visible_text(node)
            } else {
                let mut t = String::new();
                collect_del_only(node, &mut t);
                t
            };
            out.push(TrackedChange {
                id: node.get_attr("id").unwrap_or("").to_string(),
                kind: kind.to_string(),
                author: node.get_attr("author").unwrap_or("").to_string(),
                date: node.get_attr("date").unwrap_or("").to_string(),
                text,
            });
        }
        _ => {
            for child in node.children() {
                collect_changes(child, out);
            }
        }
    }
}

fn collect_del_only(node: &XmlNode, out: &mut String) {
    if node.is_element_with_local_name("delText") {
        out.push_str(&node.text_content());
        return;
    }
    for child in node.children() {
        collect_del_only(child, out);
    }
}

/// A tracked insertion or deletion.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TrackedChange {
    /// `w:id`.
    pub id: String,
    /// `insert` or `delete`.
    pub kind: String,
    /// Author.
    pub author: String,
    /// ISO date.
    pub date: String,
    /// Affected text.
    pub text: String,
}

/// Unused import guard — BodyKind is used by callers.
#[allow(dead_code)]
fn _kind(_: BodyKind) {}
