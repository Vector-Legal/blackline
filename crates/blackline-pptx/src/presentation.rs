//! Presentation façade.

use std::path::{Path, PathBuf};

use serde::Serialize;

use blackline_core::package::Package;
use blackline_core::rels;
use blackline_core::textutil::find_normalized_ci;

use crate::create::{self, CreateSpec};
use crate::edit::{self, EditOp, EditOptions, EditReport};
use crate::error::PptxError;

/// Slide listing.
#[derive(Debug, Clone, Serialize)]
pub struct SlideInfo {
    /// 1-based index.
    pub index: usize,
    /// Part name.
    pub part: String,
}

/// One slide's text view.
#[derive(Debug, Clone, Serialize)]
pub struct SlideView {
    /// 1-based index.
    pub index: usize,
    /// Text elements, 1-based.
    pub elements: Vec<String>,
}

/// Search hit.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    /// Slide index.
    pub slide: usize,
    /// Element index.
    pub element: usize,
    /// Text.
    pub text: String,
}

/// Metrics.
#[derive(Debug, Clone, Serialize)]
pub struct PptxInfo {
    /// File name.
    pub file: String,
    /// Slide count.
    pub slides: usize,
}

/// An open presentation.
pub struct Pptx {
    pub(crate) pkg: Package,
    path: Option<PathBuf>,
}

impl Pptx {
    /// Open from a path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PptxError> {
        let path = path.as_ref();
        Ok(Self {
            pkg: Package::open(path)?,
            path: Some(path.to_path_buf()),
        })
    }

    /// From bytes.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, PptxError> {
        Ok(Self {
            pkg: Package::from_bytes(bytes.as_ref())?,
            path: None,
        })
    }

    /// From a package.
    pub fn from_package(pkg: Package) -> Self {
        Self { pkg, path: None }
    }

    /// Create from a spec.
    pub fn create(spec: &CreateSpec) -> Result<Self, PptxError> {
        Ok(Self {
            pkg: create::create(spec)?,
            path: None,
        })
    }

    /// Underlying package.
    pub fn package(&self) -> &Package {
        &self.pkg
    }

    /// Mutable package.
    pub fn package_mut(&mut self) -> &mut Package {
        &mut self.pkg
    }

    /// Slide listing.
    pub fn slides(&self) -> Result<Vec<SlideInfo>, PptxError> {
        list_slides(&self.pkg)
    }

    /// Text view.
    pub fn view(&self) -> Result<Vec<SlideView>, PptxError> {
        let slides = list_slides(&self.pkg)?;
        let mut out = Vec::new();
        for s in slides {
            let doc = self.pkg.part_xml(&s.part)?;
            let mut elements = Vec::new();
            // Each txBody is one text element; join its a:t nodes.
            for body in doc.root.find_all("txBody") {
                let text: String = body
                    .find_all("t")
                    .into_iter()
                    .map(|t| t.text_content())
                    .collect();
                elements.push(text);
            }
            out.push(SlideView {
                index: s.index,
                elements,
            });
        }
        Ok(out)
    }

    /// Search.
    pub fn find(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, PptxError> {
        let mut hits = Vec::new();
        for slide in self.view()? {
            for (i, text) in slide.elements.iter().enumerate() {
                if find_normalized_ci(text, query).is_some() {
                    hits.push(SearchHit {
                        slide: slide.index,
                        element: i + 1,
                        text: text.clone(),
                    });
                    if limit > 0 && hits.len() >= limit {
                        return Ok(hits);
                    }
                }
            }
        }
        Ok(hits)
    }

    /// Metrics.
    pub fn info(&self) -> Result<PptxInfo, PptxError> {
        Ok(PptxInfo {
            file: self
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            slides: self.slides()?.len(),
        })
    }

    /// Edit.
    pub fn edit(&mut self, ops: &[EditOp], opts: &EditOptions) -> Result<EditReport, PptxError> {
        edit::apply(&mut self.pkg, ops, opts)
    }

    /// Check.
    pub fn check(&self) -> Result<blackline_core::HealthReport, PptxError> {
        Ok(blackline_core::check(&self.pkg)?)
    }

    /// Save.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), PptxError> {
        self.pkg.save(path)?;
        Ok(())
    }

    /// Bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, PptxError> {
        Ok(self.pkg.to_bytes()?)
    }
}

pub(crate) fn list_slides(pkg: &Package) -> Result<Vec<SlideInfo>, PptxError> {
    let pres = pkg.part_xml("ppt/presentation.xml")?;
    let rels = pkg.rels_for("ppt/presentation.xml")?;
    let mut out = Vec::new();
    for (i, node) in pres.root.find_all("sldId").into_iter().enumerate() {
        let rid = relationship_id(node);
        let target = rels
            .by_id(&rid)
            .map(|r| rels::resolve_target("ppt/presentation.xml", &r.target))
            .unwrap_or_else(|| format!("ppt/slides/slide{}.xml", i + 1));
        out.push(SlideInfo {
            index: i + 1,
            part: target,
        });
    }
    Ok(out)
}

fn relationship_id(node: &blackline_core::xml::XmlNode) -> String {
    if let blackline_core::xml::XmlNode::Element { attributes, .. } = node {
        for (k, v) in attributes {
            if k == "r:id" || k.ends_with(":id") && v.starts_with("rId") {
                return v.clone();
            }
        }
    }
    String::new()
}
