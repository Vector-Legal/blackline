//! Run and paragraph property helpers.

use blackline_core::xml::XmlNode;
use serde::{Deserialize, Serialize};

/// Character formatting applied to a run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunProps {
    /// Bold.
    #[serde(default)]
    pub bold: Option<bool>,
    /// Italic.
    #[serde(default)]
    pub italic: Option<bool>,
    /// Underline.
    #[serde(default)]
    pub underline: Option<bool>,
    /// Strike-through.
    #[serde(default)]
    pub strike: Option<bool>,
    /// Font name (`w:rFonts w:ascii`).
    #[serde(default)]
    pub font: Option<String>,
    /// Font size in half-points (`w:sz`).
    #[serde(default)]
    pub size: Option<u32>,
    /// Hex color without `#`.
    #[serde(default)]
    pub color: Option<String>,
    /// Superscript.
    #[serde(default)]
    pub superscript: Option<bool>,
    /// Subscript.
    #[serde(default)]
    pub subscript: Option<bool>,
}

impl RunProps {
    /// True when no property is set.
    pub fn is_empty(&self) -> bool {
        self.bold.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.strike.is_none()
            && self.font.is_none()
            && self.size.is_none()
            && self.color.is_none()
            && self.superscript.is_none()
            && self.subscript.is_none()
    }

    /// Build a `w:rPr` element, or `None` when empty.
    pub fn to_rpr(&self) -> Option<XmlNode> {
        if self.is_empty() {
            return None;
        }
        let mut rpr = XmlNode::w("rPr");
        if self.bold == Some(true) {
            rpr = rpr.with_child(XmlNode::w("b"));
        }
        if self.italic == Some(true) {
            rpr = rpr.with_child(XmlNode::w("i"));
        }
        if self.underline == Some(true) {
            rpr = rpr.with_child(XmlNode::w("u").with_attr("w:val", "single"));
        }
        if self.strike == Some(true) {
            rpr = rpr.with_child(XmlNode::w("strike"));
        }
        if let Some(font) = &self.font {
            rpr = rpr.with_child(
                XmlNode::w("rFonts")
                    .with_attr("w:ascii", font)
                    .with_attr("w:hAnsi", font),
            );
        }
        if let Some(sz) = self.size {
            rpr = rpr.with_child(XmlNode::w("sz").with_attr("w:val", sz.to_string()));
            rpr = rpr.with_child(XmlNode::w("szCs").with_attr("w:val", sz.to_string()));
        }
        if let Some(color) = &self.color {
            rpr = rpr.with_child(XmlNode::w("color").with_attr("w:val", color));
        }
        match (self.superscript, self.subscript) {
            (Some(true), _) => {
                rpr = rpr.with_child(XmlNode::w("vertAlign").with_attr("w:val", "superscript"));
            }
            (_, Some(true)) => {
                rpr = rpr.with_child(XmlNode::w("vertAlign").with_attr("w:val", "subscript"));
            }
            _ => {}
        }
        Some(rpr)
    }

    /// Merge onto an existing `w:rPr` (or create one).
    pub fn apply_to_rpr(&self, rpr: &mut XmlNode) {
        let apply_toggle = |rpr: &mut XmlNode, tag: &str, on: Option<bool>| {
            if let Some(true) = on {
                if rpr.find_child(tag).is_none() {
                    rpr.children_mut().push(XmlNode::w(tag));
                }
            } else if on == Some(false) {
                rpr.children_mut()
                    .retain(|c| !c.is_element_with_local_name(tag));
            }
        };
        apply_toggle(rpr, "b", self.bold);
        apply_toggle(rpr, "i", self.italic);
        apply_toggle(rpr, "strike", self.strike);
        if self.underline == Some(true) {
            if rpr.find_child("u").is_none() {
                rpr.children_mut()
                    .push(XmlNode::w("u").with_attr("w:val", "single"));
            }
        } else if self.underline == Some(false) {
            rpr.children_mut()
                .retain(|c| !c.is_element_with_local_name("u"));
        }
        if let Some(font) = &self.font {
            if let Some(rf) = rpr.find_child_mut("rFonts") {
                rf.set_attr("w:ascii", font);
                rf.set_attr("w:hAnsi", font);
            } else {
                rpr.children_mut().push(
                    XmlNode::w("rFonts")
                        .with_attr("w:ascii", font)
                        .with_attr("w:hAnsi", font),
                );
            }
        }
        if let Some(sz) = self.size {
            if let Some(n) = rpr.find_child_mut("sz") {
                n.set_attr("w:val", &sz.to_string());
            } else {
                rpr.children_mut()
                    .push(XmlNode::w("sz").with_attr("w:val", sz.to_string()));
            }
        }
        if let Some(color) = &self.color {
            if let Some(n) = rpr.find_child_mut("color") {
                n.set_attr("w:val", color);
            } else {
                rpr.children_mut()
                    .push(XmlNode::w("color").with_attr("w:val", color));
            }
        }
    }
}

/// Paragraph-level formatting.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParaProps {
    /// Style id (`Heading1`, `Normal`, …).
    #[serde(default)]
    pub style: Option<String>,
    /// Alignment (`left`, `center`, `right`, `both`).
    #[serde(default)]
    pub align: Option<String>,
    /// Space before, twentieths of a point.
    #[serde(default)]
    pub space_before: Option<u32>,
    /// Space after, twentieths of a point.
    #[serde(default)]
    pub space_after: Option<u32>,
}

impl ParaProps {
    /// Apply onto a paragraph (creates/updates `w:pPr`).
    pub fn apply_to_para(&self, para: &mut XmlNode) {
        let mut ppr = para
            .find_child("pPr")
            .cloned()
            .unwrap_or_else(|| XmlNode::w("pPr"));
        if let Some(style) = &self.style {
            if let Some(n) = ppr.find_child_mut("pStyle") {
                n.set_attr("w:val", style);
            } else {
                ppr.children_mut()
                    .push(XmlNode::w("pStyle").with_attr("w:val", style));
            }
        }
        if let Some(align) = &self.align {
            if let Some(n) = ppr.find_child_mut("jc") {
                n.set_attr("w:val", align);
            } else {
                ppr.children_mut()
                    .push(XmlNode::w("jc").with_attr("w:val", align));
            }
        }
        if self.space_before.is_some() || self.space_after.is_some() {
            let mut sp = ppr
                .find_child("spacing")
                .cloned()
                .unwrap_or_else(|| XmlNode::w("spacing"));
            if let Some(b) = self.space_before {
                sp.set_attr("w:before", &b.to_string());
            }
            if let Some(a) = self.space_after {
                sp.set_attr("w:after", &a.to_string());
            }
            if ppr.find_child("spacing").is_some() {
                if let Some(n) = ppr.find_child_mut("spacing") {
                    *n = sp;
                }
            } else {
                ppr.children_mut().push(sp);
            }
        }
        if para.find_child("pPr").is_some() {
            if let Some(n) = para.find_child_mut("pPr") {
                *n = ppr;
            }
        } else {
            para.children_mut().insert(0, ppr);
        }
    }
}

/// A `w:r` with optional properties and text.
pub fn build_run(text: &str, props: Option<&RunProps>) -> XmlNode {
    let mut run = XmlNode::w("r");
    if let Some(p) = props {
        if let Some(rpr) = p.to_rpr() {
            run = run.with_child(rpr);
        }
    }
    let mut t = XmlNode::w("t").with_text(text);
    if text.starts_with(|c: char| c.is_whitespace()) || text.ends_with(|c: char| c.is_whitespace())
    {
        t.set_attr("xml:space", "preserve");
    }
    run.with_child(t)
}

/// A `w:p` with a single run.
pub fn build_paragraph(text: &str, style: Option<&str>) -> XmlNode {
    let mut para = XmlNode::w("p");
    if let Some(s) = style {
        para = para
            .with_child(XmlNode::w("pPr").with_child(XmlNode::w("pStyle").with_attr("w:val", s)));
    }
    para.with_child(build_run(text, None))
}
