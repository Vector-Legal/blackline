//! Two-document redline: diff corresponding body elements and write
//! tracked changes into a copy of the original.

use blackline_core::diff::Granularity;
use blackline_core::package::Package;
use blackline_core::xml::XmlNode;

use crate::body::{find_body, find_body_mut, visible_indices, visible_text, BodyKind};
use crate::error::DocxError;
use crate::revision::{next_change_id, rebuild_with_diff};
use crate::splice::{apply_diff_surgically, apply_diff_surgically_minimal};

/// Produce a redlined copy of `original` that tracks the edits taking it
/// to `revised`.
pub fn redline(
    original: &Package,
    revised: &Package,
    author: &str,
    granularity: Granularity,
) -> Result<Package, DocxError> {
    redline_inner(original, revised, author, granularity, false)
}

/// Redline that minimizes delete+insert pairs (track module).
pub fn redline_minimal(
    original: &Package,
    revised: &Package,
    author: &str,
    granularity: Granularity,
) -> Result<Package, DocxError> {
    redline_inner(original, revised, author, granularity, true)
}

fn redline_inner(
    original: &Package,
    revised: &Package,
    author: &str,
    granularity: Granularity,
    minimal: bool,
) -> Result<Package, DocxError> {
    let mut out = original.clone();
    let main = out.main_document_part()?;
    let mut doc = out.part_xml(&main)?;
    let rev_main = revised.main_document_part()?;
    let rev_doc = revised.part_xml(&rev_main)?;

    let old_vis = visible_indices(&doc);
    let new_vis = visible_indices(&rev_doc);
    let new_body =
        find_body(&rev_doc.root).ok_or_else(|| DocxError::invalid("revised has no body"))?;

    let mut next_id = next_change_id(&doc.root);
    let n = old_vis.len().max(new_vis.len());

    // Work from the end so insertions/deletions of whole elements don't
    // shift earlier indices as we go. For the baseline we align by index
    // and rewrite paragraphs in place; extra new paragraphs are appended
    // as insertions, extra old paragraphs become whole-paragraph deletions.
    for i in 0..n {
        match (old_vis.get(i), new_vis.get(i)) {
            (Some((old_i, BodyKind::Paragraph)), Some((new_i, BodyKind::Paragraph))) => {
                let new_text = visible_text(&new_body.children()[*new_i]);
                let body =
                    find_body_mut(&mut doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
                let old_para = body
                    .children_mut()
                    .get_mut(*old_i)
                    .ok_or_else(|| DocxError::invalid("paragraph vanished during redline"))?;
                let old_text = visible_text(old_para);
                if old_text != new_text {
                    apply_redline_diff(
                        old_para,
                        &old_text,
                        &new_text,
                        author,
                        granularity,
                        &mut next_id,
                        minimal,
                    )?;
                }
            }
            (Some((old_i, BodyKind::Table)), Some((new_i, BodyKind::Table))) => {
                let new_tbl = &new_body.children()[*new_i];
                let body =
                    find_body_mut(&mut doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
                let old_tbl = body
                    .children_mut()
                    .get_mut(*old_i)
                    .ok_or_else(|| DocxError::invalid("table vanished during redline"))?;
                redline_tables(old_tbl, new_tbl, author, granularity, &mut next_id, minimal)?;
            }
            (Some(_), None) => {
                // Extra original paragraph: mark its visible text deleted.
                let body =
                    find_body_mut(&mut doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
                let old_i = old_vis[i].0;
                let old_para = body.children_mut().get_mut(old_i).unwrap();
                let old_text = visible_text(old_para);
                rebuild_with_diff(old_para, &old_text, "", author, granularity, &mut next_id);
            }
            (None, Some((new_i, BodyKind::Paragraph))) => {
                let new_text = visible_text(&new_body.children()[*new_i]);
                let mut para = crate::style::build_paragraph("", None);
                rebuild_with_diff(&mut para, "", &new_text, author, granularity, &mut next_id);
                let body =
                    find_body_mut(&mut doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
                // Insert before sectPr if present.
                let kids = body.children_mut();
                let at = kids
                    .iter()
                    .position(|c| c.is_element_with_local_name("sectPr"))
                    .unwrap_or(kids.len());
                kids.insert(at, para);
            }
            _ => {}
        }
    }

    out.set_part_xml(&main, &doc);
    Ok(out)
}

fn apply_redline_diff(
    para: &mut XmlNode,
    old_text: &str,
    new_text: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
    minimal: bool,
) -> Result<(), DocxError> {
    if minimal {
        apply_diff_surgically_minimal(para, old_text, new_text, author, granularity, next_id)
    } else {
        apply_diff_surgically(para, old_text, new_text, author, granularity, next_id)
    }
}

fn redline_tables(
    old_tbl: &mut XmlNode,
    new_tbl: &XmlNode,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
    minimal: bool,
) -> Result<(), DocxError> {
    let new_paras = paragraph_texts(new_tbl);
    let mut old_paras = Vec::new();
    collect_para_paths(old_tbl, &mut Vec::new(), &mut old_paras);
    let n = old_paras.len().min(new_paras.len());
    for i in 0..n {
        let path = old_paras[i].clone();
        let old_p = crate::body::get_mut_path(old_tbl, &path)
            .ok_or_else(|| DocxError::invalid("cell paragraph vanished"))?;
        let old_text = visible_text(old_p);
        let new_text = &new_paras[i];
        if old_text != *new_text {
            apply_redline_diff(
                old_p,
                &old_text,
                new_text,
                author,
                granularity,
                next_id,
                minimal,
            )?;
        }
    }
    Ok(())
}

fn paragraph_texts(node: &XmlNode) -> Vec<String> {
    let mut out = Vec::new();
    collect_para_texts(node, &mut out);
    out
}

fn collect_para_texts(node: &XmlNode, out: &mut Vec<String>) {
    if node.is_element_with_local_name("p") {
        out.push(visible_text(node));
        return;
    }
    for child in node.children() {
        collect_para_texts(child, out);
    }
}

fn collect_para_paths(node: &XmlNode, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    if node.is_element_with_local_name("p") {
        out.push(path.clone());
        return;
    }
    for (i, child) in node.children().iter().enumerate() {
        path.push(i);
        collect_para_paths(child, path, out);
        path.pop();
    }
}

#[allow(dead_code)]
fn _n(_: &XmlNode) {}
