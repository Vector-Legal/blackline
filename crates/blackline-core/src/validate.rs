//! Structured OOXML package health checks. No printing — callers render.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::error::CoreError;
use crate::package::Package;
use crate::rels;
use crate::xml;

/// One check in a health report.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// Stable identifier (`xml`, `rels`, `content_types`, `ids`).
    pub id: String,
    /// `pass` or `fail`.
    pub status: &'static str,
    /// Human-readable detail.
    pub detail: String,
}

/// Package health report.
#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    /// `pass` if every check passed.
    pub status: &'static str,
    /// Individual checks.
    pub checks: Vec<Check>,
}

impl HealthReport {
    /// True when every check passed.
    pub fn passed(&self) -> bool {
        self.status == "pass"
    }

    fn from_checks(checks: Vec<Check>) -> Self {
        let ok = checks.iter().all(|c| c.status == "pass");
        Self {
            status: if ok { "pass" } else { "fail" },
            checks,
        }
    }
}

/// Run the base package checks (well-formed XML, content types, relationship
/// targets, unique ids on common id-bearing elements).
pub fn check(pkg: &Package) -> Result<HealthReport, CoreError> {
    Ok(HealthReport::from_checks(vec![
        check_xml(pkg),
        check_content_types(pkg),
        check_relationships(pkg),
        check_unique_ids(pkg),
    ]))
}

fn check_xml(pkg: &Package) -> Check {
    let mut errors = Vec::new();
    for (name, bytes) in pkg.iter() {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            if let Err(e) = xml::parse_named(bytes, name) {
                errors.push(format!("{name}: {e}"));
            }
        }
    }
    if errors.is_empty() {
        Check {
            id: "xml".into(),
            status: "pass",
            detail: "all XML parts are well-formed".into(),
        }
    } else {
        Check {
            id: "xml".into(),
            status: "fail",
            detail: errors.join("; "),
        }
    }
}

fn check_content_types(pkg: &Package) -> Check {
    if !pkg.has_part("[Content_Types].xml") {
        return Check {
            id: "content_types".into(),
            status: "fail",
            detail: "[Content_Types].xml is missing".into(),
        };
    }
    match pkg.content_types() {
        Ok(_) => Check {
            id: "content_types".into(),
            status: "pass",
            detail: "[Content_Types].xml present and parseable".into(),
        },
        Err(e) => Check {
            id: "content_types".into(),
            status: "fail",
            detail: e.to_string(),
        },
    }
}

fn source_part_of_rels(rels_path: &str) -> String {
    if rels_path == "_rels/.rels" {
        return String::new();
    }
    if let Some(idx) = rels_path.find("/_rels/") {
        let dir = &rels_path[..idx];
        let file = rels_path[idx + "/_rels/".len()..]
            .strip_suffix(".rels")
            .unwrap_or(&rels_path[idx + "/_rels/".len()..]);
        return format!("{dir}/{file}");
    }
    if let Some(file) = rels_path.strip_prefix("_rels/") {
        return file.strip_suffix(".rels").unwrap_or(file).to_string();
    }
    String::new()
}

fn check_relationships(pkg: &Package) -> Check {
    let mut errors = Vec::new();
    for name in pkg.part_names() {
        if !name.ends_with(".rels") {
            continue;
        }
        let source = source_part_of_rels(name);
        let rels = match pkg.rels_for(&source) {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("{name}: {e}"));
                continue;
            }
        };
        for rel in &rels.items {
            if !is_internal_package_target(rel) {
                continue;
            }
            let target = rels::resolve_target(&source, &rel.target);
            if !pkg.has_part(&target) {
                errors.push(format!("{name}: broken target {} -> {target}", rel.id));
            }
        }
    }
    if errors.is_empty() {
        Check {
            id: "rels".into(),
            status: "pass",
            detail: "all relationship targets exist".into(),
        }
    } else {
        Check {
            id: "rels".into(),
            status: "fail",
            detail: errors.join("; "),
        }
    }
}

/// True when `Target` must exist as a package part.
///
/// Fragment identifiers (`#_ftn1`) and URL/mailto targets are not parts.
/// External `TargetMode` is also skipped.
fn is_internal_package_target(rel: &rels::Relationship) -> bool {
    if rel.target_mode.as_deref() == Some("External") {
        return false;
    }
    let t = rel.target.trim();
    if t.is_empty() || t.starts_with('#') {
        return false;
    }
    !(t.starts_with("http://") || t.starts_with("https://") || t.starts_with("mailto:"))
}

fn check_unique_ids(pkg: &Package) -> Check {
    let id_tags = [
        "comment",
        "commentRangeStart",
        "commentRangeEnd",
        "bookmarkStart",
        "bookmarkEnd",
    ];
    let mut errors = Vec::new();
    for (name, bytes) in pkg.iter() {
        if !name.ends_with(".xml") {
            continue;
        }
        let Ok(doc) = xml::parse_named(bytes, name) else {
            continue;
        };
        let mut seen: HashMap<String, HashSet<String>> = HashMap::new();
        doc.root.walk(&mut |n| {
            let ln = n.local_name();
            if id_tags.contains(&ln) {
                if let Some(id) = n.get_attr("id") {
                    let set = seen.entry(ln.to_string()).or_default();
                    if !set.insert(id.to_string()) {
                        errors.push(format!("{name}: duplicate id={id} on <{ln}>"));
                    }
                }
            }
        });
    }
    if errors.is_empty() {
        Check {
            id: "ids".into(),
            status: "pass",
            detail: "id-bearing elements are unique per part".into(),
        }
    } else {
        Check {
            id: "ids".into(),
            status: "fail",
            detail: errors.join("; "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::Package;

    #[test]
    fn empty_package_fails_content_types() {
        let pkg = Package::new();
        let report = check(&pkg).unwrap();
        assert!(!report.passed());
        assert!(report
            .checks
            .iter()
            .any(|c| c.id == "content_types" && c.status == "fail"));
    }

    #[test]
    fn fragment_and_url_targets_are_not_package_parts() {
        let fragment = crate::Relationship {
            id: "rId1".into(),
            rel_type: "http://example/hyperlink".into(),
            target: "#_ftn1".into(),
            target_mode: None,
        };
        let mailto = crate::Relationship {
            id: "rId2".into(),
            rel_type: "http://example/hyperlink".into(),
            target: "mailto:a@b.c".into(),
            target_mode: None,
        };
        let external = crate::Relationship::external(
            "rId3",
            "http://example/hyperlink",
            "http://tika.apache.org/",
        );
        let part = crate::Relationship::internal("rId4", "http://example/slide", "slide1.xml");
        assert!(!is_internal_package_target(&fragment));
        assert!(!is_internal_package_target(&mailto));
        assert!(!is_internal_package_target(&external));
        assert!(is_internal_package_target(&part));
    }
}
