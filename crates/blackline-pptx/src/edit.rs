//! Slide edit operations.

use serde::{Deserialize, Serialize};

use blackline_core::package::Package;

use crate::create::{self, CreateSpec, SlideSpec};
use crate::error::PptxError;
use crate::presentation::list_slides;

/// Options.
#[derive(Debug, Clone, Default)]
pub struct EditOptions {
    /// Best-effort.
    pub lenient: bool,
    /// Dry run.
    pub dry_run: bool,
}

/// One op.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op")]
pub enum EditOp {
    /// Replace text in a text frame.
    #[serde(rename = "set_text")]
    SetText {
        /// 1-based slide.
        slide: usize,
        /// 1-based text element (`txBody`).
        element: usize,
        /// New text.
        text: String,
    },
    /// Append a slide.
    #[serde(rename = "insert_slide")]
    InsertSlide {
        /// Texts on the new slide.
        #[serde(default)]
        texts: Vec<String>,
        /// 1-based insert position (default: append).
        #[serde(default)]
        at: Option<usize>,
    },
    /// Delete a slide.
    #[serde(rename = "delete_slide")]
    DeleteSlide {
        /// 1-based slide.
        slide: usize,
    },
}

impl EditOp {
    /// Name.
    pub fn name(&self) -> &'static str {
        match self {
            EditOp::SetText { .. } => "set_text",
            EditOp::InsertSlide { .. } => "insert_slide",
            EditOp::DeleteSlide { .. } => "delete_slide",
        }
    }
}

/// Per-op row.
#[derive(Debug, Clone, Serialize)]
pub struct OpReport {
    /// Index.
    pub index: usize,
    /// Name.
    pub op: String,
    /// Status.
    pub status: &'static str,
    /// Detail.
    pub detail: String,
}

/// Batch report.
#[derive(Debug, Clone, Serialize)]
pub struct EditReport {
    /// Applied.
    pub applied: usize,
    /// Failed.
    pub failed: usize,
    /// Mode.
    pub mode: &'static str,
    /// Rows.
    pub ops: Vec<OpReport>,
}

/// Apply.
pub fn apply(
    pkg: &mut Package,
    ops: &[EditOp],
    opts: &EditOptions,
) -> Result<EditReport, PptxError> {
    let mut reports = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        if opts.dry_run {
            reports.push(OpReport {
                index: i,
                op: op.name().into(),
                status: "applied",
                detail: "dry-run".into(),
            });
            continue;
        }
        match apply_one(pkg, op) {
            Ok(d) => reports.push(OpReport {
                index: i,
                op: op.name().into(),
                status: "applied",
                detail: d,
            }),
            Err(e) => {
                reports.push(OpReport {
                    index: i,
                    op: op.name().into(),
                    status: "failed",
                    detail: e.to_string(),
                });
                if !opts.lenient {
                    return Err(PptxError::OpFailed {
                        index: i,
                        op: op.name().into(),
                        reason: e.to_string(),
                    });
                }
            }
        }
    }
    let applied = reports.iter().filter(|r| r.status == "applied").count();
    let failed = reports.iter().filter(|r| r.status == "failed").count();
    Ok(EditReport {
        applied,
        failed,
        mode: if opts.dry_run {
            "dry-run"
        } else if opts.lenient {
            "lenient"
        } else {
            "strict"
        },
        ops: reports,
    })
}

fn apply_one(pkg: &mut Package, op: &EditOp) -> Result<String, PptxError> {
    match op {
        EditOp::SetText {
            slide,
            element,
            text,
        } => {
            let slides = list_slides(pkg)?;
            let info = slides
                .get(slide.saturating_sub(1))
                .ok_or_else(|| PptxError::invalid(format!("slide {slide} not found")))?;
            let mut doc = pkg.part_xml(&info.part)?;
            let mut bodies = Vec::new();
            collect_txbody_paths(&doc.root, &mut Vec::new(), &mut bodies);
            let path = bodies.get(element.saturating_sub(1)).ok_or_else(|| {
                PptxError::invalid(format!("element {element} not found on slide {slide}"))
            })?;
            let body = crate_get_mut(&mut doc.root, path)
                .ok_or_else(|| PptxError::invalid("txBody vanished"))?;
            set_txbody_text(body, text);
            pkg.set_part_xml(&info.part, &doc);
            Ok(format!("set slide {slide} element {element}"))
        }
        EditOp::InsertSlide { texts, at } => {
            let _ = at;
            // Rebuild presentation by creating a one-slide package and
            // grafting the slide part + relationship. Simpler: recreate
            // from current view + new slide.
            let mut spec = current_spec(pkg)?;
            let pos = at.unwrap_or(spec.slides.len() + 1).saturating_sub(1);
            spec.slides.insert(
                pos.min(spec.slides.len()),
                SlideSpec {
                    texts: texts.clone(),
                    notes: None,
                },
            );
            *pkg = create::create(&spec)?;
            Ok("inserted slide".into())
        }
        EditOp::DeleteSlide { slide } => {
            let mut spec = current_spec(pkg)?;
            if *slide == 0 || *slide > spec.slides.len() {
                return Err(PptxError::invalid(format!("slide {slide} not found")));
            }
            if spec.slides.len() == 1 {
                return Err(PptxError::invalid("cannot delete the last slide"));
            }
            spec.slides.remove(slide - 1);
            *pkg = create::create(&spec)?;
            Ok(format!("deleted slide {slide}"))
        }
    }
}

fn collect_txbody_paths(
    node: &blackline_core::xml::XmlNode,
    cur: &mut Vec<usize>,
    out: &mut Vec<Vec<usize>>,
) {
    if node.is_element_with_local_name("txBody") {
        out.push(cur.clone());
    }
    for (i, child) in node.children().iter().enumerate() {
        cur.push(i);
        collect_txbody_paths(child, cur, out);
        cur.pop();
    }
}

fn crate_get_mut<'a>(
    node: &'a mut blackline_core::xml::XmlNode,
    path: &[usize],
) -> Option<&'a mut blackline_core::xml::XmlNode> {
    let mut cur = node;
    for &i in path {
        cur = cur.try_children_mut()?.get_mut(i)?;
    }
    Some(cur)
}

fn set_txbody_text(body: &mut blackline_core::xml::XmlNode, text: &str) {
    // The slide view concatenates every `a:t` in the frame. Put the new
    // string in the first run and clear the rest so `set_text` is the
    // visible text, not a splice into the first run.
    let mut remaining = Some(text);
    if set_all_t(body, &mut remaining) && remaining.is_none() {
        return;
    }
    body.children_mut().push(
        blackline_core::xml::XmlNode::a("p").with_child(
            blackline_core::xml::XmlNode::a("r")
                .with_child(blackline_core::xml::XmlNode::a("t").with_text(text)),
        ),
    );
}

fn set_all_t(node: &mut blackline_core::xml::XmlNode, remaining: &mut Option<&str>) -> bool {
    if node.is_element_with_local_name("t") {
        match remaining.take() {
            Some(text) => node.set_text(text),
            None => node.set_text(""),
        }
        return true;
    }
    let mut found = false;
    if let Some(kids) = node.try_children_mut() {
        for kid in kids.iter_mut() {
            if set_all_t(kid, remaining) {
                found = true;
            }
        }
    }
    found
}

fn current_spec(pkg: &Package) -> Result<CreateSpec, PptxError> {
    let slides = list_slides(pkg)?;
    let mut spec = CreateSpec { slides: Vec::new() };
    for s in slides {
        let doc = pkg.part_xml(&s.part)?;
        let mut texts = Vec::new();
        for body in doc.root.find_all("txBody") {
            let text: String = body
                .find_all("t")
                .into_iter()
                .map(|t| t.text_content())
                .collect();
            texts.push(text);
        }
        spec.slides.push(SlideSpec { texts, notes: None });
    }
    Ok(spec)
}
