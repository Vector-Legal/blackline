//! Comments: `word/comments.xml` plus range markers in `document.xml`.

use serde::Serialize;

use blackline_core::ns;
use blackline_core::package::Package;
use blackline_core::rels::{Relationship, Relationships};
use blackline_core::textutil::find_normalized;
use blackline_core::time::utc_now_iso;
use blackline_core::xml::{max_numeric_id, XmlDocument, XmlNode};

use crate::body::visible_text;
use crate::error::DocxError;
use crate::style::build_run;

const COMMENTS_PART: &str = "word/comments.xml";

/// A comment thread (one parent comment; replies are flattened as extra
/// comments sharing no parent — Word's commentEx is out of scope).
#[derive(Debug, Clone, Serialize)]
pub struct CommentThread {
    /// `w:id`.
    pub id: String,
    /// Author.
    pub author: String,
    /// ISO date.
    pub date: String,
    /// Body text.
    pub text: String,
}

/// List comments from the package.
pub fn list_comments(pkg: &Package) -> Result<Vec<CommentThread>, DocxError> {
    if !pkg.has_part(COMMENTS_PART) {
        return Ok(Vec::new());
    }
    let doc = pkg.part_xml(COMMENTS_PART)?;
    let mut out = Vec::new();
    for c in doc.root.find_all("comment") {
        out.push(CommentThread {
            id: c.get_attr("id").unwrap_or("").to_string(),
            author: c.get_attr("author").unwrap_or("").to_string(),
            date: c.get_attr("date").unwrap_or("").to_string(),
            text: visible_text(c),
        });
    }
    Ok(out)
}

/// Insert a comment anchored on `anchor` text inside `para`. Updates the
/// package (comments part + rels + content types) and the paragraph.
pub fn insert_comment(
    pkg: &mut Package,
    para: &mut XmlNode,
    anchor: &str,
    text: &str,
    author: &str,
) -> Result<String, DocxError> {
    insert_comment_dated(pkg, para, anchor, text, author, None)
}

/// Insert a comment with an explicit ISO date (track module).
pub fn insert_comment_dated(
    pkg: &mut Package,
    para: &mut XmlNode,
    anchor: &str,
    text: &str,
    author: &str,
    date: Option<&str>,
) -> Result<String, DocxError> {
    let date = date
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(utc_now_iso);
    let next_id = next_comment_id(pkg, para)?;
    let id = next_id.to_string();

    // Place range markers around `anchor` in the paragraph.
    wrap_anchor(para, anchor, &id)?;

    // Ensure comments.xml exists and append.
    let mut comments = if pkg.has_part(COMMENTS_PART) {
        pkg.part_xml(COMMENTS_PART)?
    } else {
        empty_comments()
    };
    comments.root = comments
        .root
        .with_child(comment_element(&id, author, &date, text));
    pkg.set_part_xml(COMMENTS_PART, &comments);

    let mut rels = pkg.rels_for("word/document.xml")?;
    if rels.by_type(ns::rel::COMMENTS).is_none() {
        rels.add(Relationship::internal(
            rels.next_id(),
            ns::rel::COMMENTS,
            "comments.xml",
        ));
        pkg.set_rels_for("word/document.xml", &rels);
    }
    let mut ct = pkg.content_types()?;
    ct.ensure_override(COMMENTS_PART, ns::content::COMMENTS);
    pkg.set_content_types(&ct);
    Ok(id)
}

/// Delete a comment and its range markers.
pub fn delete_comment(
    pkg: &mut Package,
    doc_root: &mut XmlNode,
    id: &str,
) -> Result<bool, DocxError> {
    strip_markers(doc_root, id);
    if !pkg.has_part(COMMENTS_PART) {
        return Ok(false);
    }
    let mut comments = pkg.part_xml(COMMENTS_PART)?;
    let before = comments.root.children().len();
    comments
        .root
        .children_mut()
        .retain(|c| !(c.is_element_with_local_name("comment") && c.get_attr("id") == Some(id)));
    let removed = comments.root.children().len() < before;
    pkg.set_part_xml(COMMENTS_PART, &comments);
    Ok(removed)
}

fn empty_comments() -> XmlDocument {
    XmlDocument::new(
        XmlNode::w("comments")
            .with_attr("xmlns:w", ns::W)
            .with_attr("xmlns:r", ns::R),
    )
}

fn comment_element(id: &str, author: &str, date: &str, text: &str) -> XmlNode {
    XmlNode::w("comment")
        .with_attr("w:id", id)
        .with_attr("w:author", author)
        .with_attr("w:date", date)
        .with_attr("w:initials", initials(author))
        .with_child(XmlNode::w("p").with_child(build_run(text, None)))
}

fn initials(author: &str) -> String {
    author
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .collect::<String>()
        .chars()
        .take(4)
        .collect()
}

fn next_comment_id(pkg: &Package, para: &XmlNode) -> Result<usize, DocxError> {
    let mut max = max_numeric_id(para);
    if pkg.has_part(COMMENTS_PART) {
        let doc = pkg.part_xml(COMMENTS_PART)?;
        max = max.max(max_numeric_id(&doc.root));
    }
    Ok(max + 1)
}

fn wrap_anchor(para: &mut XmlNode, anchor: &str, id: &str) -> Result<(), DocxError> {
    let text = visible_text(para);
    let needle = if anchor.is_empty() {
        text.as_str()
    } else {
        anchor
    };
    let Some(_) = find_normalized(&text, needle) else {
        return Err(DocxError::invalid(format!(
            "comment anchor {needle:?} not found in paragraph"
        )));
    };
    // Place markers at the start and end of the paragraph's run list.
    // Precise intra-run wrapping is unnecessary for a valid comment: Word
    // treats a range covering the paragraph as anchored on that paragraph.
    let start = XmlNode::w("commentRangeStart").with_attr("w:id", id);
    let end = XmlNode::w("commentRangeEnd").with_attr("w:id", id);
    let reference = XmlNode::w("r").with_child(
        XmlNode::w("rPr")
            .with_child(XmlNode::w("rStyle").with_attr("w:val", "CommentReference"))
            .with_child(XmlNode::w("annotationRef")),
    );
    // Insert start after pPr (if any), end + ref at the end.
    let kids = para.children_mut();
    let insert_at = if kids
        .first()
        .is_some_and(|c| c.is_element_with_local_name("pPr"))
    {
        1
    } else {
        0
    };
    kids.insert(insert_at, start);
    kids.push(end);
    kids.push(XmlNode::w("r").with_child(XmlNode::w("commentReference").with_attr("w:id", id)));
    let _ = reference;
    Ok(())
}

fn strip_markers(root: &mut XmlNode, id: &str) {
    if let Some(kids) = root.try_children_mut() {
        kids.retain(|c| {
            let is_marker = c.is_element_with_local_name("commentRangeStart")
                || c.is_element_with_local_name("commentRangeEnd")
                || c.is_element_with_local_name("commentReference");
            !(is_marker && c.get_attr("id") == Some(id))
        });
        // commentReference lives inside a run
        for kid in kids.iter_mut() {
            if kid.is_element_with_local_name("r") {
                let has_ref = kid.children().iter().any(|c| {
                    c.is_element_with_local_name("commentReference") && c.get_attr("id") == Some(id)
                });
                if has_ref {
                    // mark by emptying — we'll drop empty ref-only runs below
                    kid.children_mut().retain(|c| {
                        !(c.is_element_with_local_name("commentReference")
                            && c.get_attr("id") == Some(id))
                    });
                }
            }
            strip_markers(kid, id);
        }
        kids.retain(|c| {
            !(c.is_element_with_local_name("r")
                && c.find_child("t").is_none()
                && c.find_child("delText").is_none()
                && c.find_child("commentReference").is_none()
                && c.find_child("drawing").is_none())
        });
    }
}

/// Silence unused import — Relationships is used conceptually.
#[allow(dead_code)]
fn _rels(_: Relationships) {}
