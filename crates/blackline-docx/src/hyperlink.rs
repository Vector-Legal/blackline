//! Hyperlinks (`w:hyperlink` + an external relationship).

use serde::Serialize;

use blackline_core::ns;
use blackline_core::package::Package;
use blackline_core::rels::Relationship;
use blackline_core::textutil::find_normalized;
use blackline_core::xml::XmlNode;

use crate::body::visible_text;
use crate::error::DocxError;
use crate::style::build_run;

/// A hyperlink in a story part.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Hyperlink {
    /// Display text.
    pub text: String,
    /// Target URL (`http:`, `mailto:`, …).
    pub target: String,
    /// Package part that owns the relationship.
    pub part: String,
}

/// List external hyperlinks in `part` (usually `word/document.xml`).
pub fn list_hyperlinks(pkg: &Package, part: &str) -> Result<Vec<Hyperlink>, DocxError> {
    if !pkg.has_part(part) {
        return Ok(Vec::new());
    }
    let doc = pkg.part_xml(part)?;
    let rels = pkg.rels_for(part)?;
    let mut out = Vec::new();
    collect(&doc.root, &rels, part, &mut out);
    Ok(out)
}

fn collect(
    node: &XmlNode,
    rels: &blackline_core::rels::Relationships,
    part: &str,
    out: &mut Vec<Hyperlink>,
) {
    if node.is_element_with_local_name("hyperlink") {
        if let Some(rid) = node.get_attr("id") {
            if let Some(rel) = rels.by_id(rid) {
                out.push(Hyperlink {
                    text: visible_text(node),
                    target: rel.target.clone(),
                    part: part.to_string(),
                });
            }
        }
        return;
    }
    for child in node.children() {
        collect(child, rels, part, out);
    }
}

/// Point an existing hyperlink whose display text contains `text` at `url`,
/// or wrap that text in a new `w:hyperlink`.
pub fn set_hyperlink(
    pkg: &mut Package,
    part: &str,
    para: &mut XmlNode,
    text: &str,
    url: &str,
) -> Result<String, DocxError> {
    if find_normalized(&visible_text(para), text).is_none() {
        return Err(DocxError::invalid(format!(
            "text {text:?} not found for hyperlink"
        )));
    }
    if let Some(rid) = existing_hyperlink_rid(para, text) {
        let mut rels = pkg.rels_for(part)?;
        if let Some(rel) = rels.items.iter_mut().find(|r| r.id == rid) {
            rel.target = url.to_string();
            rel.target_mode = Some("External".into());
            rel.rel_type = ns::rel::HYPERLINK.to_string();
        }
        pkg.set_rels_for(part, &rels);
        return Ok(format!("updated hyperlink {rid}"));
    }

    let mut rels = pkg.rels_for(part)?;
    let rid = rels.add(Relationship::external(
        String::new(),
        ns::rel::HYPERLINK,
        url,
    ));
    pkg.set_rels_for(part, &rels);
    wrap_text(para, text, &rid)?;
    Ok(format!("created hyperlink {rid}"))
}

fn existing_hyperlink_rid(para: &XmlNode, text: &str) -> Option<String> {
    let mut found = None;
    para.walk(&mut |n| {
        if n.is_element_with_local_name("hyperlink")
            && find_normalized(&visible_text(n), text).is_some()
        {
            if let Some(id) = n.get_attr("id") {
                found = Some(id.to_string());
            }
        }
    });
    found
}

fn wrap_text(para: &mut XmlNode, text: &str, rid: &str) -> Result<(), DocxError> {
    // Prefer wrapping a top-level run whose visible text contains `text`.
    if let Some(kids) = para.try_children_mut() {
        for i in 0..kids.len() {
            if kids[i].is_element_with_local_name("r")
                && find_normalized(&visible_text(&kids[i]), text).is_some()
            {
                let run = kids.remove(i);
                let link = XmlNode::w("hyperlink")
                    .with_attr("r:id", rid)
                    .with_child(styled_link_run(run));
                kids.insert(i, link);
                return Ok(());
            }
        }
    }
    // Fallback: append a linked run.
    let run = styled_link_run(build_run(text, None));
    para.children_mut().push(
        XmlNode::w("hyperlink")
            .with_attr("r:id", rid)
            .with_child(run),
    );
    Ok(())
}

fn styled_link_run(mut run: XmlNode) -> XmlNode {
    if run.find_child("rPr").is_none() {
        let rpr =
            XmlNode::w("rPr").with_child(XmlNode::w("rStyle").with_attr("w:val", "InternetLink"));
        run.children_mut().insert(0, rpr);
    }
    run
}
