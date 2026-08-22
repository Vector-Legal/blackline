//! `[Content_Types].xml` — the OPC content-type map.

use crate::error::CoreError;
use crate::ns;
use crate::xml::{self, XmlDocument, XmlNode};

/// One default mapping (by file extension).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultType {
    /// Extension without the dot (`"xml"`, `"rels"`, `"png"`).
    pub extension: String,
    /// MIME type.
    pub content_type: String,
}

/// One override mapping (by part name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideType {
    /// Part name with a leading slash (`"/word/document.xml"`).
    pub part_name: String,
    /// MIME type.
    pub content_type: String,
}

/// The package content-type map.
#[derive(Debug, Clone, Default)]
pub struct ContentTypes {
    /// Extension defaults.
    pub defaults: Vec<DefaultType>,
    /// Per-part overrides.
    pub overrides: Vec<OverrideType>,
}

impl ContentTypes {
    /// The defaults every Office package needs (`rels`, `xml`).
    pub fn office_defaults() -> Self {
        let mut ct = Self::default();
        ct.ensure_default("rels", ns::content::RELS);
        ct.ensure_default("xml", ns::content::XML);
        ct
    }

    /// Parse `[Content_Types].xml`.
    pub fn parse(bytes: &[u8]) -> Result<Self, CoreError> {
        let doc = xml::parse(bytes)?;
        let mut defaults = Vec::new();
        let mut overrides = Vec::new();
        for n in doc.root.children() {
            match n.local_name() {
                "Default" => {
                    if let (Some(ext), Some(ct)) =
                        (n.get_attr("Extension"), n.get_attr("ContentType"))
                    {
                        defaults.push(DefaultType {
                            extension: ext.to_string(),
                            content_type: ct.to_string(),
                        });
                    }
                }
                "Override" => {
                    if let (Some(pn), Some(ct)) =
                        (n.get_attr("PartName"), n.get_attr("ContentType"))
                    {
                        overrides.push(OverrideType {
                            part_name: pn.to_string(),
                            content_type: ct.to_string(),
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(Self {
            defaults,
            overrides,
        })
    }

    /// Serialize.
    pub fn to_xml(&self) -> XmlDocument {
        let mut root = XmlNode::element("Types").with_attr("xmlns", ns::CT);
        for d in &self.defaults {
            root = root.with_child(
                XmlNode::element("Default")
                    .with_attr("Extension", &d.extension)
                    .with_attr("ContentType", &d.content_type),
            );
        }
        for o in &self.overrides {
            root = root.with_child(
                XmlNode::element("Override")
                    .with_attr("PartName", &o.part_name)
                    .with_attr("ContentType", &o.content_type),
            );
        }
        XmlDocument::new(root)
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_xml().to_bytes()
    }

    /// Ensure an extension default exists.
    pub fn ensure_default(&mut self, extension: &str, content_type: &str) {
        if !self.defaults.iter().any(|d| d.extension == extension) {
            self.defaults.push(DefaultType {
                extension: extension.to_string(),
                content_type: content_type.to_string(),
            });
        }
    }

    /// Ensure an override exists (or update it).
    pub fn ensure_override(&mut self, part_name: &str, content_type: &str) {
        let name = if part_name.starts_with('/') {
            part_name.to_string()
        } else {
            format!("/{part_name}")
        };
        if let Some(o) = self.overrides.iter_mut().find(|o| o.part_name == name) {
            o.content_type = content_type.to_string();
            return;
        }
        self.overrides.push(OverrideType {
            part_name: name,
            content_type: content_type.to_string(),
        });
    }

    /// Remove the override for a part.
    pub fn remove_override(&mut self, part_name: &str) {
        let name = if part_name.starts_with('/') {
            part_name.to_string()
        } else {
            format!("/{part_name}")
        };
        self.overrides.retain(|o| o.part_name != name);
    }

    /// Look up the content type of a part (override, then extension default).
    pub fn content_type_of(&self, part_name: &str) -> Option<&str> {
        let name = if part_name.starts_with('/') {
            part_name.to_string()
        } else {
            format!("/{part_name}")
        };
        if let Some(o) = self.overrides.iter().find(|o| o.part_name == name) {
            return Some(o.content_type.as_str());
        }
        let ext = part_name.rsplit_once('.').map(|(_, e)| e)?;
        self.defaults
            .iter()
            .find(|d| d.extension == ext)
            .map(|d| d.content_type.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_override() {
        let mut ct = ContentTypes::office_defaults();
        ct.ensure_override("word/document.xml", ns::content::DOCUMENT);
        assert_eq!(
            ct.content_type_of("word/document.xml"),
            Some(ns::content::DOCUMENT)
        );
        assert_eq!(ct.content_type_of("foo.xml"), Some(ns::content::XML));
        let again = ContentTypes::parse(&ct.to_bytes()).unwrap();
        assert_eq!(again.overrides.len(), 1);
    }
}
