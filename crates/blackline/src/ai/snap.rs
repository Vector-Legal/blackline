//! Snap model ops onto text that actually exists in the paragraph.
//!
//! Phi-3 concatenates several `N|` lines into one `old` and copies the
//! prompt ellipsis. Apply then fails with `text "…" not found`. Each
//! ` | `-joined span becomes its own replace, using the document's spelling.

use std::collections::BTreeSet;

use blackline_core::textutil::find_normalized_ci;

use super::plan::{Op, Plan};

/// Rewrite replace ops so `old` is a substring of a real paragraph.
pub(crate) fn snap_plan(plan: &mut Plan, lines: &[String]) {
    let mut out = Vec::with_capacity(plan.ops.len());
    let mut claimed = BTreeSet::new();
    for op in std::mem::take(&mut plan.ops) {
        match op {
            Op::Replace { index, old, new } => {
                let snapped = expand_replace(lines, index, &old, &new, &mut claimed);
                if snapped.is_empty() {
                    out.push(Op::Replace { index, old, new });
                } else {
                    out.extend(snapped);
                }
            }
            other => out.push(other),
        }
    }
    plan.ops = out;
}

fn expand_replace(
    lines: &[String],
    index: u32,
    old: &str,
    new: &str,
    claimed: &mut BTreeSet<u32>,
) -> Vec<Op> {
    let old_clean = strip_prompt_junk(old);
    let new_clean = strip_prompt_junk(new);
    let old_parts = split_mashed(&old_clean);
    if old_parts.is_empty() {
        return Vec::new();
    }
    let new_parts = split_mashed(&new_clean);
    let mut ops = Vec::new();
    let mut cursor = index;
    for (i, part) in old_parts.iter().enumerate() {
        let model_new = new_parts.get(i).map(String::as_str).unwrap_or("");
        for needle in needles_for_part(lines, cursor, part) {
            let Some((idx, actual, rewritten)) = snap_replace(lines, cursor, &needle, model_new)
            else {
                continue;
            };
            if !claimed.insert(idx) {
                continue;
            }
            ops.push(Op::Replace {
                index: idx,
                old: actual,
                new: rewritten,
            });
            cursor = idx.saturating_add(1);
        }
    }
    ops
}

fn needles_for_part(lines: &[String], index: u32, part: &str) -> Vec<String> {
    if part.chars().count() < 2 {
        return Vec::new();
    }
    if locate(lines, index, part).is_some() {
        return vec![part.to_string()];
    }
    let bits: Vec<String> = part
        .split(" / ")
        .map(strip_prompt_junk)
        .filter(|s| s.chars().count() >= 2)
        .collect();
    if bits.len() > 1 {
        bits
    } else {
        vec![part.to_string()]
    }
}

fn split_mashed(s: &str) -> Vec<String> {
    s.split(" | ")
        .map(strip_prompt_junk)
        .filter(|p| p.chars().count() >= 2)
        .collect()
}

fn snap_replace(
    lines: &[String],
    index: u32,
    old: &str,
    new: &str,
) -> Option<(u32, String, String)> {
    let cleaned = strip_prompt_junk(old);
    let segment = cleaned.split(" | ").next().unwrap_or(&cleaned).trim();
    let needle = leading_label(segment);
    if needle.chars().count() < 2 {
        return None;
    }
    let (idx, actual) = locate(lines, index, &needle)?;
    let new = rewrite_new(&actual, new);
    Some((idx, actual, new))
}

/// Fill-in tails (`Services: [Name …]`) are not a stable `old`.
fn leading_label(s: &str) -> String {
    if let Some(i) = s.find('[') {
        let head = s[..i].trim();
        if head.chars().count() >= 2 {
            return head.to_string();
        }
    }
    s.to_string()
}

fn locate(lines: &[String], index: u32, needle: &str) -> Option<(u32, String)> {
    if let Some(i) = usize::try_from(index).ok().and_then(|n| n.checked_sub(1)) {
        if let Some(line) = lines.get(i) {
            if let Some(matched) = match_in(line_body(line), needle) {
                return Some((index, matched));
            }
        }
    }
    let start = usize::try_from(index.saturating_sub(1)).unwrap_or(0);
    for (i, line) in lines.iter().enumerate().skip(start) {
        if let Some(matched) = match_in(line_body(line), needle) {
            let idx = u32::try_from(i.saturating_add(1)).ok()?;
            return Some((idx, matched));
        }
    }
    for (i, line) in lines.iter().enumerate().take(start) {
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
    let n = strip_prompt_junk(model_new);
    let n = n.split(" | ").next().unwrap_or(&n).trim();
    let letters: String = n.chars().filter(|c| c.is_alphabetic()).collect();
    if !letters.is_empty() && letters.chars().all(|c| c.is_uppercase()) {
        return actual_old.to_uppercase();
    }
    if n.eq_ignore_ascii_case(actual_old) {
        return n.to_string();
    }
    n.to_string()
}

fn strip_prompt_junk(s: &str) -> String {
    s.trim()
        .trim_end_matches('…')
        .trim_end_matches("...")
        .trim_end_matches('_')
        .trim()
        .to_string()
}

/// Numbered view prefix `12| text`. A pipe inside the paragraph (`Address: |`)
/// must stay; apply snaps against `text_lines()`, not the prompt.
fn line_body(line: &str) -> &str {
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return line;
    }
    let rest = line.get(digits..).unwrap_or("");
    rest.strip_prefix("| ")
        .or_else(|| rest.strip_prefix('|'))
        .unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::{line_body, snap_plan, snap_replace};
    use crate::ai::plan::{Op, Plan};

    fn numbered() -> Vec<String> {
        vec![
            "1| SaaS Services Order Form".into(),
            "2| Customer:".into(),
            "3| Contact:".into(),
            "4| Address:".into(),
        ]
    }

    /// Same shape as `Docx::text_lines()` — no `N|` prefix, pipes are content.
    fn apply_lines() -> Vec<String> {
        vec![
            "SaaS Services Order Form".into(),
            "Customer:".into(),
            "Contact: /".into(),
            "Address: |".into(),
            "Phone: / E-Mail: /".into(),
            "Services: [Name and briefly describe services here]".into(),
        ]
    }

    fn replace_triples(plan: &Plan) -> Vec<(u32, String, String)> {
        plan.ops
            .iter()
            .filter_map(|op| match op {
                Op::Replace { index, old, new } => Some((*index, old.clone(), new.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn mashed_pipe_lines_snap_to_the_first_field() {
        let (idx, old, new) = snap_replace(
            &numbered(),
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
            &numbered(),
            1,
            "saas services order form",
            "SAAS SERVICES ORDER FORM",
        )
        .unwrap();
        assert_eq!(old, "SaaS Services Order Form");
        assert_eq!(new, "SAAS SERVICES ORDER FORM");
    }

    #[test]
    fn numbered_prefix_is_stripped_document_pipe_is_not() {
        assert_eq!(line_body("2| Customer:"), "Customer:");
        assert_eq!(line_body("Address: |"), "Address: |");
        assert_eq!(line_body("Customer:"), "Customer:");
    }

    #[test]
    fn mashed_yc_header_expands_to_each_paragraph() {
        let mut plan = Plan {
            ops: vec![Op::Replace {
                index: 1,
                old: "Customer: | Contact: / Address: | Phone: / E-Mail: / Services: [Name and briefly describe services here] ____…".into(),
                new: "CUSTOMER: | CONTACT: / ADDRESS: | PHONE: / E-MAIL: / SERVICES: [NAME AND BRIEFLY DESCRIBE SERVICES HERE] ____…".into(),
            }],
        };
        snap_plan(&mut plan, &apply_lines());
        assert_eq!(
            replace_triples(&plan),
            vec![
                (2, "Customer:".into(), "CUSTOMER:".into()),
                (3, "Contact:".into(), "CONTACT:".into()),
                (4, "Address:".into(), "ADDRESS:".into()),
                (5, "Phone:".into(), "PHONE:".into()),
                (6, "Services:".into(), "SERVICES:".into()),
            ]
        );
    }

    #[test]
    fn snap_plan_expands_mashed_numbered_lines() {
        let mut plan = Plan {
            ops: vec![Op::Replace {
                index: 1,
                old: "Customer: | Contact: …".into(),
                new: "CUSTOMER: | CONTACT: …".into(),
            }],
        };
        snap_plan(&mut plan, &numbered());
        assert_eq!(
            replace_triples(&plan),
            vec![
                (2, "Customer:".into(), "CUSTOMER:".into()),
                (3, "Contact:".into(), "CONTACT:".into()),
            ]
        );
    }
}
