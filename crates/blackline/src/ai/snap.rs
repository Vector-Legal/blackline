//! Snap model ops onto text that actually exists in the paragraph.
//!
//! Phi-3 concatenates several `N|` lines into one `old` and copies the
//! prompt ellipsis. Apply then fails with `text "…" not found`. Each
//! ` | `-joined span becomes its own replace, using the document's spelling.

use std::collections::BTreeSet;

use blackline_core::textutil::find_normalized_ci;

use super::plan::{Op, Plan, Position};

/// Rewrite replace ops so `old` is a substring of a real paragraph.
pub(crate) fn snap_plan(plan: &mut Plan, lines: &[String]) {
    let mut out = Vec::with_capacity(plan.ops.len());
    let mut claimed = BTreeSet::new();
    for op in std::mem::take(&mut plan.ops) {
        match op {
            Op::Replace { index, old, new } => {
                match expand_replace(lines, index, &old, &new, &mut claimed) {
                    SnapOutcome::Keep => out.push(Op::Replace { index, old, new }),
                    SnapOutcome::Replace(ops) => out.extend(ops),
                }
            }
            Op::Insert {
                index,
                position,
                text,
            } => {
                if let Some(op) = snap_insert(lines, index, position, text, &mut claimed) {
                    out.push(op);
                }
            }
            other => out.push(other),
        }
    }
    plan.ops = out;
}

/// A Word table is one view index whose prompt text joins cells with ` | `.
/// Several replaces on that index are valid; a leftover op with the wrong
/// paragraph number is not.
enum SnapOutcome {
    Keep,
    Replace(Vec<Op>),
}

fn expand_replace(
    lines: &[String],
    index: u32,
    old: &str,
    new: &str,
    claimed: &mut BTreeSet<(u32, String)>,
) -> SnapOutcome {
    let old_clean = strip_prompt_junk(old);
    let new_clean = strip_prompt_junk(new);
    let old_parts = split_mashed(&old_clean);
    if old_parts.is_empty() {
        return SnapOutcome::Keep;
    }
    let new_parts = split_mashed(&new_clean);
    let mut ops = Vec::new();
    let mut cursor = index;
    let mut duplicate = false;
    for (i, part) in old_parts.iter().enumerate() {
        let model_new = new_parts.get(i).map(String::as_str).unwrap_or("");
        for needle in needles_for_part(lines, cursor, part) {
            let Some((idx, actual, rewritten)) = snap_replace(lines, cursor, &needle, model_new)
            else {
                continue;
            };
            let key = (idx, actual.to_lowercase());
            if !claimed.insert(key) {
                duplicate = true;
                continue;
            }
            ops.push(Op::Replace {
                index: idx,
                old: actual,
                new: rewritten,
            });
            cursor = idx;
        }
    }
    if ops.is_empty() {
        // Do not keep an unsnappable replace (`H1:` from the view
        // prefix). Strict apply aborts the whole batch on the first miss.
        let _ = duplicate;
        return SnapOutcome::Replace(Vec::new());
    }
    SnapOutcome::Replace(ops)
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

/// Phi-3 often emits `insert` after a line instead of `replace`. Track
/// `before`/`after` then dies (`needs match`) and strict apply writes nothing.
/// A same-length n-gram that shares the last word becomes a replace
/// (`thirty days` → `sixty days`), including a prefix of a mashed insert
/// (`sixty days of invoice date`) and a hit on a neighbor line. A short
/// leftover phrase with no swap is dropped so it does not glue onto the
/// previous clause. A real sentence still inserts at `end`.
fn snap_insert(
    lines: &[String],
    index: u32,
    position: Position,
    text: String,
    claimed: &mut BTreeSet<(u32, String)>,
) -> Option<Op> {
    let text = strip_prompt_junk(&text);
    if let Some((idx, old, new)) = all_caps_insert_as_replace(lines, index, &text) {
        let key = (idx, old.to_lowercase());
        if !claimed.insert(key) {
            return None;
        }
        return Some(Op::Replace {
            index: idx,
            old,
            new,
        });
    }
    if let Some((idx, old, new)) = insert_as_replace(lines, index, &text, claimed) {
        let key = (idx, old.to_lowercase());
        if !claimed.insert(key) {
            return None;
        }
        return Some(Op::Replace {
            index: idx,
            old,
            new,
        });
    }
    if is_short_leftover_phrase(&text) {
        return None;
    }
    let position = match position {
        Position::Before | Position::After => Position::End,
        other => other,
    };
    Some(Op::Insert {
        index,
        position,
        text,
    })
}

fn insert_as_replace(
    lines: &[String],
    index: u32,
    text: &str,
    claimed: &BTreeSet<(u32, String)>,
) -> Option<(u32, String, String)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }
    for n in (2..=words.len()).rev() {
        let last = words[n - 1];
        if !contentful_word(last) {
            continue;
        }
        let prefix = words[..n]
            .join(" ")
            .trim_end_matches(|c: char| !c.is_alphanumeric())
            .to_string();
        if let Some((idx, old)) = find_swap_ngram(lines, index, n, last, &prefix, claimed) {
            return Some((idx, old, prefix));
        }
    }
    None
}

fn index_claimed(claimed: &BTreeSet<(u32, String)>, idx: u32) -> bool {
    claimed.iter().any(|(i, _)| *i == idx)
}

/// Target line, later lines, then the previous line. Do not wrap to
/// article 1 — that retargets a clause another op already changed.
fn find_swap_ngram(
    lines: &[String],
    index: u32,
    n: usize,
    last: &str,
    new_phrase: &str,
    claimed: &BTreeSet<(u32, String)>,
) -> Option<(u32, String)> {
    let start = usize::try_from(index.saturating_sub(1)).unwrap_or(0);
    let prev = start.checked_sub(1).into_iter();
    for i in std::iter::once(start)
        .chain(start.saturating_add(1)..lines.len())
        .chain(prev)
    {
        let idx = u32::try_from(i.saturating_add(1)).ok()?;
        if index_claimed(claimed, idx) {
            continue;
        }
        if let Some(old) = ngram_sharing_last(line_body(&lines[i]), n, last, new_phrase) {
            return Some((idx, old));
        }
    }
    None
}

fn ngram_sharing_last(body: &str, n: usize, last: &str, new_phrase: &str) -> Option<String> {
    let line_words: Vec<&str> = body.split_whitespace().collect();
    let last_key = word_key(last);
    for window in line_words.windows(n) {
        if word_key(window.last()?) != last_key {
            continue;
        }
        let old = window
            .join(" ")
            .trim_end_matches(|c: char| !c.is_alphanumeric())
            .to_string();
        if word_key(&old) == word_key(new_phrase) {
            continue;
        }
        return Some(old);
    }
    None
}

fn word_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_short_leftover_phrase(text: &str) -> bool {
    let t = text.trim();
    let words: Vec<&str> = t.split_whitespace().collect();
    if words.is_empty() || words.len() > 6 {
        return false;
    }
    let core = t.trim_end_matches(['.', '?', '!']).trim();
    if core.contains(['.', '?', '!']) {
        return false;
    }
    let starts_capital = words[0].chars().next().is_some_and(|c| c.is_uppercase());
    let ends_sentence = t.ends_with(['.', '?', '!']);
    !(starts_capital && ends_sentence)
}

fn contentful_word(word: &str) -> bool {
    word_key(word).chars().count() >= 4
}

/// All-caps insert of a sentence is Phi-3 doing `all caps first` as a
/// duplicate paragraph. Promote it to a first-word replace on the target
/// line so the original clause is not appended in caps.
fn all_caps_insert_as_replace(
    lines: &[String],
    index: u32,
    text: &str,
) -> Option<(u32, String, String)> {
    if !is_all_caps_text(text) {
        return None;
    }
    let i = usize::try_from(index).ok()?.checked_sub(1)?;
    let first = first_word(line_body(lines.get(i)?))?;
    if first.is_empty() {
        return None;
    }
    Some((index, first.clone(), first.to_uppercase()))
}

fn is_all_caps_text(s: &str) -> bool {
    let letters: String = s.chars().filter(|c| c.is_alphabetic()).collect();
    !letters.is_empty() && letters.chars().all(|c| c.is_uppercase())
}

fn first_word(s: &str) -> Option<String> {
    let raw = s.split_whitespace().next()?;
    let trimmed = raw.trim_end_matches(|c: char| !c.is_alphanumeric());
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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

/// How far a mashed `Customer: | Contact:` replace may walk.
const LOCATE_FORWARD: usize = 8;
const LOCATE_BACK: usize = 2;

fn locate(lines: &[String], index: u32, needle: &str) -> Option<(u32, String)> {
    if let Some(i) = usize::try_from(index).ok().and_then(|n| n.checked_sub(1)) {
        if let Some(line) = lines.get(i) {
            if let Some(matched) = match_in(line_body(line), needle) {
                return Some((index, matched));
            }
        }
    }
    let start = usize::try_from(index.saturating_sub(1)).unwrap_or(0);
    let lo = start.saturating_sub(LOCATE_BACK);
    let hi = start
        .saturating_add(LOCATE_FORWARD)
        .min(lines.len().saturating_sub(1));
    for i in (start.saturating_add(1)..=hi).chain(lo..start) {
        if let Some(matched) = match_in(line_body(&lines[i]), needle) {
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
    let rest = rest
        .strip_prefix("| ")
        .or_else(|| rest.strip_prefix('|'))
        .unwrap_or(line);
    strip_heading_prefix(rest)
}

/// `bl docx view` prints `H1: ` in front of a heading. That prefix is
/// not in the paragraph, so a copied `old` of `H1:` must not be kept.
fn strip_heading_prefix(s: &str) -> &str {
    let t = s.trim_start();
    let b = t.as_bytes();
    if b.len() >= 4 && b[0] == b'H' && b[1].is_ascii_digit() && b[2] == b':' && b[3] == b' ' {
        return t.get(4..).unwrap_or(t);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::{line_body, snap_plan, snap_replace};
    use crate::ai::plan::{Op, Plan, Position};

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
        assert_eq!(
            line_body("1| H1: SaaS Services Order Form"),
            "SaaS Services Order Form"
        );
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
                (5, "E-Mail:".into(), "E-MAIL:".into()),
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

    /// YC header is one `w:tbl` (one view index). Phi-3 still emits
    /// `Contact:` against the next paragraph; that leftover must drop.
    #[test]
    fn table_row_keeps_each_field_and_drops_wrong_index() {
        let table = "Customer: | Contact: / | Address: | Phone: / E-Mail: / | Services: [Name]";
        let lines = vec![
            "SaaS Services Order Form".into(),
            table.into(),
            "This SaaS Services Agreement is entered into".into(),
        ];
        let mut plan = Plan {
            ops: vec![
                Op::Replace {
                    index: 2,
                    old: "Customer: | Contact: / Address: | Phone: ____…".into(),
                    new: "CUSTOMER: | CONTACT: / ADDRESS: | PHONE: ____…".into(),
                },
                Op::Replace {
                    index: 3,
                    old: "Contact:".into(),
                    new: "CONTACT:".into(),
                },
            ],
        };
        snap_plan(&mut plan, &lines);
        assert_eq!(
            replace_triples(&plan),
            vec![
                (2, "Customer:".into(), "CUSTOMER:".into()),
                (2, "Contact:".into(), "CONTACT:".into()),
                (2, "Address:".into(), "ADDRESS:".into()),
                (2, "Phone:".into(), "PHONE:".into()),
            ]
        );
    }

    #[test]
    fn insert_after_swaps_a_same_length_phrase() {
        let lines = vec!["Fees are due within thirty days of invoice date.".into()];
        let mut plan = Plan {
            ops: vec![Op::Insert {
                index: 1,
                position: Position::After,
                text: "sixty days".into(),
            }],
        };
        snap_plan(&mut plan, &lines);
        assert_eq!(
            replace_triples(&plan),
            vec![(1, "thirty days".into(), "sixty days".into())]
        );
    }

    #[test]
    fn insert_after_without_a_swap_moves_to_end() {
        let lines = vec!["Fees are due within thirty days of invoice date.".into()];
        let mut plan = Plan {
            ops: vec![Op::Insert {
                index: 1,
                position: Position::After,
                text: "See Exhibit A.".into(),
            }],
        };
        snap_plan(&mut plan, &lines);
        match &plan.ops[..] {
            [Op::Insert {
                index: 1,
                position: Position::End,
                text,
            }] => assert_eq!(text, "See Exhibit A."),
            other => panic!("expected insert at end, got {other:?}"),
        }
    }

    #[test]
    fn mashed_insert_uses_a_prefix_swap() {
        let lines = vec!["Either party may terminate after thirty days notice.".into()];
        let mut plan = Plan {
            ops: vec![Op::Insert {
                index: 1,
                position: Position::After,
                text: "sixty days of invoice date".into(),
            }],
        };
        snap_plan(&mut plan, &lines);
        assert_eq!(
            replace_triples(&plan),
            vec![(1, "thirty days".into(), "sixty days".into())]
        );
    }

    #[test]
    fn leftover_insert_on_a_neighbor_swaps_the_next_line() {
        let lines = vec![
            "Provider shall implement commercially reasonable security measures.".into(),
            "Either party may terminate after thirty days notice.".into(),
        ];
        let mut plan = Plan {
            ops: vec![Op::Insert {
                index: 1,
                position: Position::After,
                text: "sixty days notice".into(),
            }],
        };
        snap_plan(&mut plan, &lines);
        assert_eq!(
            replace_triples(&plan),
            vec![(2, "thirty days notice".into(), "sixty days notice".into())]
        );
    }

    #[test]
    fn insert_does_not_wrap_onto_a_claimed_early_clause() {
        let lines = vec![
            "Either party may terminate after thirty days notice.".into(),
            "Fees are due within thirty days of invoice.".into(),
            "Limitation of liability shall not exceed fees paid.".into(),
        ];
        let mut plan = Plan {
            ops: vec![
                Op::Replace {
                    index: 1,
                    old: "thirty".into(),
                    new: "sixty".into(),
                },
                Op::Insert {
                    index: 3,
                    position: Position::After,
                    text: "sixty days notice.".into(),
                },
            ],
        };
        snap_plan(&mut plan, &lines);
        let triples = replace_triples(&plan);
        assert_eq!(triples[0], (1, "thirty".into(), "sixty".into()));
        assert!(
            !triples
                .iter()
                .any(|(i, old, _)| *i == 1 && old.contains("notice")),
            "wrapped onto the already-claimed first clause: {triples:?}"
        );
    }

    #[test]
    fn short_leftover_insert_with_no_swap_is_dropped() {
        let lines = vec!["Limitation of liability shall not exceed fees paid.".into()];
        let mut plan = Plan {
            ops: vec![Op::Insert {
                index: 1,
                position: Position::After,
                text: "sixty days notice.".into(),
            }],
        };
        snap_plan(&mut plan, &lines);
        assert!(plan.ops.is_empty(), "{:?}", plan.ops);
    }

    #[test]
    fn mashed_of_suffix_does_not_rewrite_unrelated_claims() {
        let lines = vec![
            "Indemnity covers third-party claims of intellectual property infringement.".into(),
        ];
        let mut plan = Plan {
            ops: vec![Op::Insert {
                index: 1,
                position: Position::After,
                text: "sixty days of invoice date.".into(),
            }],
        };
        snap_plan(&mut plan, &lines);
        assert!(plan.ops.is_empty(), "{:?}", plan.ops);
    }

    #[test]
    fn replace_does_not_walk_to_a_far_article() {
        let mut lines = vec!["Export controls apply to all software and technical data.".into()];
        lines.extend(std::iter::repeat_n(
            "Unrelated clause without the needle.".into(),
            20,
        ));
        lines.push("Export controls apply to all software and technical data.".into());
        let mut plan = Plan {
            ops: vec![Op::Replace {
                index: 5,
                old: "Export controls apply to all software and technical data.".into(),
                new: "EXPORT CONTROLS APPLY TO ALL SOFTWARE AND TECHNICAL DATA.".into(),
            }],
        };
        snap_plan(&mut plan, &lines);
        assert!(
            !replace_triples(&plan).iter().any(|(i, _, _)| *i >= 20),
            "walked to a far article: {:?}",
            replace_triples(&plan)
        );
    }

    #[test]
    fn view_heading_prefix_replace_is_dropped() {
        let lines = vec!["1| H1: SaaS Services Order Form".into()];
        let mut plan = Plan {
            ops: vec![Op::Replace {
                index: 1,
                old: "H1:".into(),
                new: "H1:".into(),
            }],
        };
        snap_plan(&mut plan, &lines);
        assert!(plan.ops.is_empty(), "{:?}", plan.ops);
    }

    #[test]
    fn all_caps_insert_becomes_first_word_replace() {
        let lines = vec!["Indemnity covers third-party claims.".into()];
        let mut plan = Plan {
            ops: vec![Op::Insert {
                index: 1,
                position: Position::After,
                text: "INDEMNITY COVERS THIRD-PARTY CLAIMS.".into(),
            }],
        };
        snap_plan(&mut plan, &lines);
        assert_eq!(
            replace_triples(&plan),
            vec![(1, "Indemnity".into(), "INDEMNITY".into())]
        );
    }
}
