//! Body-element addressing. One 1-based index space shared by view, find,
//! and every edit op: non-empty paragraphs, tables, and SDT blocks count.

use blackline_core::xml::{XmlDocument, XmlNode};

use crate::error::DocxError;

/// Kind of a visible body element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// A paragraph (`w:p`).
    Paragraph,
    /// A table (`w:tbl`).
    Table,
    /// A structured document tag (`w:sdt`).
    Sdt,
}

fn is_story_root(node: &XmlNode) -> bool {
    matches!(node.local_name(), "body" | "hdr" | "ftr")
}

/// Locate the story container: `w:body`, `w:hdr`, or `w:ftr`.
pub fn find_body(root: &XmlNode) -> Option<&XmlNode> {
    if is_story_root(root) {
        return Some(root);
    }
    root.find_all("body")
        .into_iter()
        .next()
        .or_else(|| root.find_all("hdr").into_iter().next())
        .or_else(|| root.find_all("ftr").into_iter().next())
}

/// Mutable story container (`w:body` / `w:hdr` / `w:ftr`).
pub fn find_body_mut(root: &mut XmlNode) -> Option<&mut XmlNode> {
    if is_story_root(root) {
        return Some(root);
    }
    let path = first_path(root, "body")
        .or_else(|| first_path(root, "hdr"))
        .or_else(|| first_path(root, "ftr"))?;
    get_mut_path(root, &path)
}

/// First descendant path with this local name.
pub fn first_path(node: &XmlNode, local: &str) -> Option<Vec<usize>> {
    for (i, child) in node.children().iter().enumerate() {
        if child.is_element_with_local_name(local) {
            return Some(vec![i]);
        }
        if let Some(mut p) = first_path(child, local) {
            p.insert(0, i);
            return Some(p);
        }
    }
    None
}

/// Follow a child-index path.
pub fn get_mut_path<'a>(node: &'a mut XmlNode, path: &[usize]) -> Option<&'a mut XmlNode> {
    let mut cur = node;
    for &i in path {
        cur = cur.try_children_mut()?.get_mut(i)?;
    }
    Some(cur)
}

/// Immutable follow.
pub fn get_path<'a>(node: &'a XmlNode, path: &[usize]) -> Option<&'a XmlNode> {
    let mut cur = node;
    for &i in path {
        cur = cur.children().get(i)?;
    }
    Some(cur)
}

/// Visible (view-indexed) body children: `(body_child_index, kind)`.
pub fn visible_indices(doc: &XmlDocument) -> Vec<(usize, BodyKind)> {
    let Some(body) = find_body(&doc.root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, child) in body.children().iter().enumerate() {
        match child.local_name() {
            "p" => {
                if paragraph_is_visible(child) {
                    out.push((i, BodyKind::Paragraph));
                }
            }
            "tbl" => out.push((i, BodyKind::Table)),
            "sdt" => out.push((i, BodyKind::Sdt)),
            _ => {}
        }
    }
    out
}

/// A paragraph is visible when it has text, a heading style, or numbering.
pub fn paragraph_is_visible(para: &XmlNode) -> bool {
    if !visible_text(para).is_empty() {
        return true;
    }
    heading_level(para).is_some()
}

/// 1-based view index → body child index.
pub fn resolve_view_index(doc: &XmlDocument, index: usize) -> Result<usize, DocxError> {
    if index == 0 {
        return Err(DocxError::invalid("index is 1-based; 0 is not valid"));
    }
    let vis = visible_indices(doc);
    vis.get(index - 1).map(|(i, _)| *i).ok_or_else(|| {
        DocxError::invalid(format!("index {index} out of range (max {})", vis.len()))
    })
}

/// Find the first visible element whose text contains `needle`
/// (smart-quote normalized, case-insensitive).
pub fn resolve_match(doc: &XmlDocument, needle: &str) -> Result<usize, DocxError> {
    use blackline_core::textutil::{find_normalized_ci, normalize_quotes};
    let needle = normalize_quotes(needle);
    let vis = visible_indices(doc);
    let body = find_body(&doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
    for (view, (child_i, _)) in vis.iter().enumerate() {
        let text = visible_text(&body.children()[*child_i]);
        if find_normalized_ci(&text, &needle).is_some() {
            return Ok(view + 1);
        }
    }
    Err(DocxError::invalid(format!(
        "no body element matching {needle:?}"
    )))
}

/// Heading level 1–9 from `HeadingN` / `heading N` style, if any.
pub fn heading_level(para: &XmlNode) -> Option<usize> {
    let ppr = para.find_child("pPr")?;
    let style = ppr.find_child("pStyle")?.get_attr("val")?;
    let s = style.to_ascii_lowercase().replace(' ', "");
    s.strip_prefix("heading")?.parse().ok()
}

/// Visible text: `w:t` outside `w:del`. Insertions are included.
pub fn visible_text(node: &XmlNode) -> String {
    let mut out = String::new();
    collect_visible(node, &mut out);
    out
}

fn collect_visible(node: &XmlNode, out: &mut String) {
    if node.is_element_with_local_name("del") {
        return;
    }
    if node.is_element_with_local_name("t") {
        out.push_str(&node.text_content());
        return;
    }
    for child in node.children() {
        collect_visible(child, out);
    }
}

/// Deleted text (`w:delText`) plus visible text — used by raw view and
/// reject-all reconstruction.
#[allow(dead_code)]
pub fn all_markup_text(node: &XmlNode) -> String {
    let mut out = String::new();
    collect_all_markup(node, &mut out);
    out
}

#[allow(dead_code)]
fn collect_all_markup(node: &XmlNode, out: &mut String) {
    match node.local_name() {
        "t" | "delText" => out.push_str(&node.text_content()),
        _ => {
            for child in node.children() {
                collect_all_markup(child, out);
            }
        }
    }
}

/// Original text: skip insertions, include deletions.
pub fn original_text(node: &XmlNode) -> String {
    let mut out = String::new();
    collect_original(node, &mut out);
    out
}

fn collect_original(node: &XmlNode, out: &mut String) {
    if node.is_element_with_local_name("ins") {
        return;
    }
    if node.is_element_with_local_name("t") || node.is_element_with_local_name("delText") {
        out.push_str(&node.text_content());
        return;
    }
    for child in node.children() {
        collect_original(child, out);
    }
}

/// Revised text: include insertions, skip deletions. Same as [`visible_text`].
#[allow(dead_code)]
pub fn revised_text(node: &XmlNode) -> String {
    visible_text(node)
}

/// Table cell texts, row-major.
pub fn table_cells(tbl: &XmlNode) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for tr in tbl.find_all("tr") {
        // find_all is recursive so we only want direct-ish rows; filter by
        // walking tr children for tc.
        let mut cells = Vec::new();
        for child in tr.children() {
            if child.is_element_with_local_name("tc") {
                cells.push(visible_text(child));
            }
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    rows
}
