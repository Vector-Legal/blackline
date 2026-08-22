//! DOCX health check: base package checks plus revision / comment pairing.

use blackline_core::package::Package;
use blackline_core::validate::{self, Check, HealthReport};
use serde::Serialize;

use crate::body::original_text;
use crate::error::DocxError;
use crate::revision::settle_all;

/// Full DOCX report.
#[derive(Debug, Clone, Serialize)]
pub struct DocxHealth {
    /// `pass` / `fail`.
    pub status: &'static str,
    /// Individual checks.
    pub checks: Vec<Check>,
}

impl DocxHealth {
    /// True when every check passed.
    pub fn passed(&self) -> bool {
        self.status == "pass"
    }
}

/// Check `pkg`. When `original` is given, rejecting every tracked change
/// must reproduce the original's visible (pre-revision) text.
pub fn check(pkg: &Package, original: Option<&Package>) -> Result<DocxHealth, DocxError> {
    let base = validate::check(pkg)?;
    let mut checks = base.checks;
    checks.push(check_revisions(pkg));
    checks.push(check_comment_pairing(pkg));
    if let Some(orig) = original {
        checks.push(check_redline_integrity(pkg, orig)?);
    }
    let ok = checks.iter().all(|c| c.status == "pass");
    Ok(DocxHealth {
        status: if ok { "pass" } else { "fail" },
        checks,
    })
}

fn check_revisions(pkg: &Package) -> Check {
    let Ok(main) = pkg.main_document_part() else {
        return Check {
            id: "revisions".into(),
            status: "fail",
            detail: "no main document part".into(),
        };
    };
    let Ok(doc) = pkg.part_xml(&main) else {
        return Check {
            id: "revisions".into(),
            status: "fail",
            detail: "document.xml failed to parse".into(),
        };
    };
    let mut errors: Vec<String> = Vec::new();
    doc.root.walk(&mut |n| {
        if n.is_element_with_local_name("del") {
            n.walk(&mut |c| {
                if c.is_element_with_local_name("t") {
                    errors.push("w:t inside w:del".into());
                }
            });
        }
        if n.is_element_with_local_name("ins") {
            n.walk(&mut |c| {
                if c.is_element_with_local_name("delText") {
                    errors.push("w:delText inside w:ins".into());
                }
            });
        }
    });
    if errors.is_empty() {
        Check {
            id: "revisions".into(),
            status: "pass",
            detail: "revision markup is well nested".into(),
        }
    } else {
        Check {
            id: "revisions".into(),
            status: "fail",
            detail: errors.join("; "),
        }
    }
}

fn check_comment_pairing(pkg: &Package) -> Check {
    let Ok(main) = pkg.main_document_part() else {
        return Check {
            id: "comments".into(),
            status: "fail",
            detail: "no main document".into(),
        };
    };
    let Ok(doc) = pkg.part_xml(&main) else {
        return Check {
            id: "comments".into(),
            status: "fail",
            detail: "parse failed".into(),
        };
    };
    use std::collections::HashSet;
    let mut starts = HashSet::new();
    let mut ends = HashSet::new();
    let mut refs = HashSet::new();
    doc.root.walk(&mut |n| {
        if let Some(id) = n.get_attr("id") {
            match n.local_name() {
                "commentRangeStart" => {
                    starts.insert(id.to_string());
                }
                "commentRangeEnd" => {
                    ends.insert(id.to_string());
                }
                "commentReference" => {
                    refs.insert(id.to_string());
                }
                _ => {}
            }
        }
    });
    let xml_ids = comment_ids_from_part(pkg);
    if starts == ends && starts == refs && starts == xml_ids {
        Check {
            id: "comments".into(),
            status: "pass",
            detail: format!("{} comment range(s) paired", starts.len()),
        }
    } else {
        Check {
            id: "comments".into(),
            status: "fail",
            detail: format!(
                "unpaired comment markers starts={starts:?} ends={ends:?} refs={refs:?} comments.xml={xml_ids:?}"
            ),
        }
    }
}

fn comment_ids_from_part(pkg: &Package) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();
    if !pkg.has_part("word/comments.xml") {
        return ids;
    }
    let Ok(doc) = pkg.part_xml("word/comments.xml") else {
        return ids;
    };
    for c in doc.root.find_all("comment") {
        if let Some(id) = c.get_attr("id") {
            ids.insert(id.to_string());
        }
    }
    ids
}

fn check_redline_integrity(pkg: &Package, original: &Package) -> Result<Check, DocxError> {
    let main = pkg.main_document_part()?;
    let mut edited = pkg.part_xml(&main)?;
    settle_all(&mut edited.root, false, None);
    let got = original_text(&edited.root);

    let orig_main = original.main_document_part()?;
    let orig_doc = original.part_xml(&orig_main)?;
    let want = crate::body::visible_text(&orig_doc.root);

    if normalize(&got) == normalize(&want) {
        Ok(Check {
            id: "redline".into(),
            status: "pass",
            detail: "reject-all reproduces the original text".into(),
        })
    } else {
        Ok(Check {
            id: "redline".into(),
            status: "fail",
            detail: "reject-all does not reproduce the original text".into(),
        })
    }
}

fn normalize(s: &str) -> String {
    blackline_core::textutil::normalize_quotes(s)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl From<HealthReport> for DocxHealth {
    fn from(report: HealthReport) -> Self {
        Self {
            status: report.status,
            checks: report.checks,
        }
    }
}
