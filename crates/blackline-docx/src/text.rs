//! Numbered text view and outline.

use serde::Serialize;

use crate::body::{
    find_body, heading_level, paragraph_is_visible, table_cells, visible_indices, visible_text,
    BodyKind,
};
use crate::error::DocxError;
use blackline_core::xml::{XmlDocument, XmlNode};

/// One line of the numbered view.
#[derive(Debug, Clone, Serialize)]
pub struct ViewLine {
    /// 1-based view index.
    pub index: usize,
    /// `paragraph`, `table`, or `sdt`.
    pub kind: &'static str,
    /// Heading level, when this is a heading paragraph.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<usize>,
    /// Rendered text.
    pub text: String,
}

/// Heading outline entry.
#[derive(Debug, Clone, Serialize)]
pub struct OutlineEntry {
    /// 1-based view index.
    pub index: usize,
    /// Heading level (1–9).
    pub level: usize,
    /// Heading text.
    pub text: String,
}

/// Numbered view lines.
pub fn view_lines(doc: &XmlDocument, raw: bool) -> Result<Vec<ViewLine>, DocxError> {
    let body = find_body(&doc.root).ok_or_else(|| DocxError::invalid("no body"))?;
    let vis = visible_indices(doc);
    let mut lines = Vec::new();
    for (view, (child_i, kind)) in vis.iter().enumerate() {
        let child = &body.children()[*child_i];
        let index = view + 1;
        let (kind_s, text, heading) = match kind {
            BodyKind::Paragraph => {
                let text = if raw {
                    raw_paragraph_text(child)
                } else {
                    visible_text(child)
                };
                ("paragraph", text, heading_level(child))
            }
            BodyKind::Table => {
                let text = render_table(child);
                ("table", text, None)
            }
            BodyKind::Sdt => {
                let alias = child
                    .find_all("alias")
                    .into_iter()
                    .find_map(|n| n.get_attr("val").map(str::to_string));
                let inner = visible_text(child);
                let text = match alias {
                    Some(a) => format!("[SDT {a}] {inner}"),
                    None => format!("[SDT] {inner}"),
                };
                ("sdt", text, None)
            }
        };
        lines.push(ViewLine {
            index,
            kind: kind_s,
            heading,
            text,
        });
    }
    Ok(lines)
}

/// `{index}| {text}` rendering used by the CLI's default view.
pub fn render_view(lines: &[ViewLine]) -> Vec<String> {
    lines
        .iter()
        .map(|l| {
            let mut prefix = format!("{}|", l.index);
            if let Some(h) = l.heading {
                prefix = format!("{prefix} H{h}:");
            }
            format!("{prefix} {}", l.text)
        })
        .collect()
}

/// Outline of heading paragraphs.
pub fn outline(doc: &XmlDocument) -> Result<Vec<OutlineEntry>, DocxError> {
    Ok(view_lines(doc, false)?
        .into_iter()
        .filter_map(|l| {
            l.heading.map(|level| OutlineEntry {
                index: l.index,
                level,
                text: l.text,
            })
        })
        .collect())
}

fn render_table(tbl: &XmlNode) -> String {
    table_cells(tbl)
        .into_iter()
        .map(|row| row.join(" | "))
        .collect::<Vec<_>>()
        .join(" / ")
}

fn raw_paragraph_text(para: &XmlNode) -> String {
    let mut parts = Vec::new();
    collect_raw(para, &mut parts);
    parts.join("")
}

fn collect_raw(node: &XmlNode, parts: &mut Vec<String>) {
    match node.local_name() {
        "ins" => {
            let id = node.get_attr("id").unwrap_or("?");
            let author = node.get_attr("author").unwrap_or("?");
            let text = visible_text(node);
            parts.push(format!("[INS id={id} author={author}]{text}[/INS]"));
        }
        "del" => {
            let id = node.get_attr("id").unwrap_or("?");
            let author = node.get_attr("author").unwrap_or("?");
            let mut text = String::new();
            collect_del_text(node, &mut text);
            parts.push(format!("[DEL id={id} author={author}]{text}[/DEL]"));
        }
        "r" => {
            // Only emit if not already wrapped — collect_raw is called from
            // children of ins/del too. When we hit ins/del we don't recurse
            // into their children via the default path.
            let text = visible_text(node);
            if !text.is_empty() {
                parts.push(text);
            }
        }
        _ => {
            for child in node.children() {
                collect_raw(child, parts);
            }
        }
    }
}

fn collect_del_text(node: &XmlNode, out: &mut String) {
    if node.is_element_with_local_name("delText") || node.is_element_with_local_name("t") {
        out.push_str(&node.text_content());
        return;
    }
    for child in node.children() {
        collect_del_text(child, out);
    }
}

/// Used by body::paragraph_is_visible tests via re-export.
#[allow(dead_code)]
pub fn is_visible_para(p: &XmlNode) -> bool {
    paragraph_is_visible(p)
}
