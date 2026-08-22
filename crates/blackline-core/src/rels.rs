//! OPC relationships (`.rels` parts).

use crate::error::CoreError;
use crate::ns;
use crate::xml::{self, XmlDocument, XmlNode};

/// One relationship record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    /// `rId` (e.g. `"rId1"`).
    pub id: String,
    /// Relationship type URI.
    pub rel_type: String,
    /// Target path, relative to the source part (or absolute from package root).
    pub target: String,
    /// `TargetMode`, usually `None` (internal) or `Some("External")`.
    pub target_mode: Option<String>,
}

impl Relationship {
    /// An internal relationship.
    pub fn internal(
        id: impl Into<String>,
        rel_type: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            rel_type: rel_type.into(),
            target: target.into(),
            target_mode: None,
        }
    }

    /// An external relationship (hyperlinks, …).
    pub fn external(
        id: impl Into<String>,
        rel_type: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            rel_type: rel_type.into(),
            target: target.into(),
            target_mode: Some("External".into()),
        }
    }
}

/// A `.rels` part.
#[derive(Debug, Clone, Default)]
pub struct Relationships {
    /// Relationship records, in document order.
    pub items: Vec<Relationship>,
}

impl Relationships {
    /// Parse a `.rels` XML part.
    pub fn parse(bytes: &[u8]) -> Result<Self, CoreError> {
        let doc = xml::parse(bytes)?;
        let mut items = Vec::new();
        for node in doc.root.find_all("Relationship") {
            let id = node.get_attr("Id").unwrap_or("").to_string();
            let rel_type = node.get_attr("Type").unwrap_or("").to_string();
            let target = node.get_attr("Target").unwrap_or("").to_string();
            let target_mode = node.get_attr("TargetMode").map(str::to_string);
            items.push(Relationship {
                id,
                rel_type,
                target,
                target_mode,
            });
        }
        Ok(Self { items })
    }

    /// Serialize to a `.rels` XML document.
    pub fn to_xml(&self) -> XmlDocument {
        let mut root = XmlNode::element("Relationships").with_attr("xmlns", ns::PKG_REL);
        for rel in &self.items {
            let mut n = XmlNode::element("Relationship")
                .with_attr("Id", &rel.id)
                .with_attr("Type", &rel.rel_type)
                .with_attr("Target", &rel.target);
            if let Some(mode) = &rel.target_mode {
                n.set_attr("TargetMode", mode);
            }
            root = root.with_child(n);
        }
        XmlDocument::new(root)
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_xml().to_bytes()
    }

    /// Next unused `rIdN`.
    pub fn next_id(&self) -> String {
        let mut max = 0usize;
        for rel in &self.items {
            if let Some(n) = rel.id.strip_prefix("rId") {
                if let Ok(v) = n.parse::<usize>() {
                    if v > max {
                        max = v;
                    }
                }
            }
        }
        format!("rId{}", max + 1)
    }

    /// Add a relationship, allocating an id when `rel.id` is empty.
    pub fn add(&mut self, mut rel: Relationship) -> String {
        if rel.id.is_empty() {
            rel.id = self.next_id();
        }
        let id = rel.id.clone();
        self.items.push(rel);
        id
    }

    /// Find by id.
    pub fn by_id(&self, id: &str) -> Option<&Relationship> {
        self.items.iter().find(|r| r.id == id)
    }

    /// Find the first relationship of this type.
    pub fn by_type(&self, rel_type: &str) -> Option<&Relationship> {
        self.items.iter().find(|r| r.rel_type == rel_type)
    }

    /// All relationships of this type.
    pub fn all_of_type(&self, rel_type: &str) -> Vec<&Relationship> {
        self.items
            .iter()
            .filter(|r| r.rel_type == rel_type)
            .collect()
    }

    /// Remove by id. Returns the removed record.
    pub fn remove(&mut self, id: &str) -> Option<Relationship> {
        let pos = self.items.iter().position(|r| r.id == id)?;
        Some(self.items.remove(pos))
    }
}

/// Path of the `.rels` part that belongs to `part`.
///
/// `word/document.xml` → `word/_rels/document.xml.rels`
/// package root (`""`) → `_rels/.rels`
pub fn rels_path_for(part: &str) -> String {
    if part.is_empty() || part == "/" {
        return "_rels/.rels".into();
    }
    let part = part.trim_start_matches('/');
    match part.rfind('/') {
        Some(i) => {
            let (dir, file) = part.split_at(i);
            format!("{dir}/_rels{file}.rels")
        }
        None => format!("_rels/{part}.rels"),
    }
}

/// Resolve a relationship target against its source part, returning a
/// package-absolute part name (no leading slash).
pub fn resolve_target(source_part: &str, target: &str) -> String {
    if target.starts_with('/') {
        return target.trim_start_matches('/').to_string();
    }
    let source = source_part.trim_start_matches('/');
    let dir = match source.rfind('/') {
        Some(i) => &source[..i],
        None => "",
    };
    normalize_path(dir, target)
}

fn normalize_path(dir: &str, target: &str) -> String {
    let mut stack: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    stack.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rels_path() {
        assert_eq!(rels_path_for(""), "_rels/.rels");
        assert_eq!(
            rels_path_for("word/document.xml"),
            "word/_rels/document.xml.rels"
        );
        assert_eq!(
            rels_path_for("xl/workbook.xml"),
            "xl/_rels/workbook.xml.rels"
        );
    }

    #[test]
    fn resolve_relative() {
        assert_eq!(
            resolve_target("word/document.xml", "comments.xml"),
            "word/comments.xml"
        );
        assert_eq!(
            resolve_target("ppt/slides/slide1.xml", "../slideLayouts/slideLayout1.xml"),
            "ppt/slideLayouts/slideLayout1.xml"
        );
        assert_eq!(
            resolve_target("word/document.xml", "/word/media/x.png"),
            "word/media/x.png"
        );
    }

    #[test]
    fn parse_and_roundtrip() {
        let xml = br#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://example/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
        let rels = Relationships::parse(xml).unwrap();
        assert_eq!(rels.items.len(), 1);
        assert_eq!(rels.next_id(), "rId2");
        let again = Relationships::parse(&rels.to_bytes()).unwrap();
        assert_eq!(again.items[0].target, "word/document.xml");
    }
}
