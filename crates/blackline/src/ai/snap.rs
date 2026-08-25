//! Snap model ops onto text that actually exists in the numbered view.
//!
//! Phi-3 concatenates several `N|` lines into one `old` and copies the
//! prompt ellipsis. Apply then fails with `text "…" not found`. We keep
//! the first real span and the document's spelling.

use blackline_core::textutil::find_normalized_ci;

use super::plan::{Op, Plan};

/// Rewrite replace ops so `old` is a substring of the target paragraph.
pub(crate) fn snap_plan(plan: &mut Plan, lines: &[String]) {
    for op in &mut plan.ops {
        if let Op::Replace { index, old, new } = op {
            if let Some((idx, o, n)) = snap_replace(lines, *index, old, new) {
                *index = idx;
                *old = o;
                *new = n;
            }
        }
    }
}

fn snap_replace(
    lines: &[String],
    index: u32,
    old: &str,
    new: &str,
) -> Option<(u32, String, String)> {
    let needle = first_view_segment(&strip_prompt_junk(old));
    if needle.chars().count() < 2 {
        return None;
    }
    let (idx, actual) = locate(lines, index, &needle)?;
    let new = rewrite_new(&actual, new);
    Some((idx, actual, new))
}

fn locate(lines: &[String], index: u32, needle: &str) -> Option<(u32, String)> {
    if let Some(i) = usize::try_from(index).ok().and_then(|n| n.checked_sub(1)) {
        if let Some(line) = lines.get(i) {
            if let Some(matched) = match_in(line_body(line), needle) {
                return Some((index, matched));
            }
        }
    }
    for (i, line) in lines.iter().enumerate() {
        if let Some(matched) = match_in(line_body(line), needle) {
            let idx = u32::try_from(i.saturating_add(1)).ok()?;
            return Some((idx, matched));
        }
    }
    None
}

fn match_in(haystack: &str, needle: &str) -> Option<String> {
    find_normalized_ci(haystack, needle).map(|(_, matched)| matched)
}

fn rewrite_new(actual_old: &str, model_new: &str) -> String {
    let n = first_view_segment(&strip_prompt_junk(model_new));
    let letters: String = n.chars().filter(|c| c.is_alphabetic()).collect();
    if !letters.is_empty() && letters.chars().all(|c| c.is_uppercase()) {
        return actual_old.to_uppercase();
    }
    if n.eq_ignore_ascii_case(actual_old) {
        return n;
    }
    n
}

fn strip_prompt_junk(s: &str) -> String {
    s.trim()
        .trim_end_matches('…')
        .trim_end_matches("...")
        .trim()
        .to_string()
}

/// Model often joins `2| Customer:` and `3| Contact:` as `Customer: | Contact:`.
fn first_view_segment(s: &str) -> String {
    s.split(" | ").next().unwrap_or(s).trim().to_string()
}

fn line_body(line: &str) -> &str {
    if let Some((_, rest)) = line.split_once("| ") {
        return rest;
    }
    if let Some((_, rest)) = line.split_once('|') {
        return rest.trim_start();
    }
    line
}

#[cfg(test)]
mod tests {
    use super::{snap_plan, snap_replace};
    use crate::ai::plan::{Op, Plan};

    fn lines() -> Vec<String> {
        vec![
            "1| SaaS Services Order Form".into(),
            "2| Customer:".into(),
            "3| Contact:".into(),
            "4| Address:".into(),
        ]
    }

    #[test]
    fn mashed_pipe_lines_snap_to_the_first_field() {
        let (idx, old, new) = snap_replace(
            &lines(),
            1,
            "Customer: | Contact: / Address: | Phone: ____…",
            "CUSTOMER: | CONTACT: / ADDRESS: | PHONE: ____…",
        )
        .unwrap();
        assert_eq!(idx, 2);
        assert_eq!(old, "Customer:");
        assert_eq!(new, "CUSTOMER:");
    }

    #[test]
    fn all_caps_uses_document_spelling() {
        let (_, old, new) = snap_replace(
            &lines(),
            1,
            "saas services order form",
            "SAAS SERVICES ORDER FORM",
        )
        .unwrap();
        assert_eq!(old, "SaaS Services Order Form");
        assert_eq!(new, "SAAS SERVICES ORDER FORM");
    }

    #[test]
    fn snap_plan_rewrites_ops() {
        let mut plan = Plan {
            ops: vec![Op::Replace {
                index: 1,
                old: "Customer: | Contact: …".into(),
                new: "CUSTOMER: | CONTACT: …".into(),
            }],
        };
        snap_plan(&mut plan, &lines());
        match &plan.ops[0] {
            Op::Replace { index, old, new } => {
                assert_eq!(*index, 2);
                assert_eq!(old, "Customer:");
                assert_eq!(new, "CUSTOMER:");
            }
            other => panic!("{other:?}"),
        }
    }
}
