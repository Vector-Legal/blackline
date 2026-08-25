//! Numbered document view fed to the model. Same index space as `bl docx view`.
//!
//! The model still addresses 1-based indexes, but the user does not have to
//! pass `--from` / `--to`. When those flags are omitted, phrases from the
//! instruction are searched in the full file and only those hits (plus a
//! neighbor) go into the prompt. `every paragraph` walks the file in chunks.

use std::collections::BTreeSet;
use std::path::Path;

use blackline_docx::Docx;
use blackline_pptx::Pptx;
use blackline_xlsx::Xlsx;

use super::error::AiError;
use super::format::Format;

/// Soft cap on characters stuffed into the prompt. TinyLlama is 2k tokens
/// and Phi-3 is 4k. A long file is windowed to instruction hits first;
/// document-wide instructions walk the file in chunks. `--from` / `--to`
/// is the explicit override.
pub const CONTEXT_CHARS: usize = 2_500;

/// Prompt lines are abbreviated so a 2k-character legal paragraph does
/// not eat the whole window and tempt the model to copy it into `old`.
pub const PROMPT_LINE_CHARS: usize = 120;

/// Paragraphs kept on each side of an instruction hit.
const HIT_PAD: usize = 1;

/// A file, opened and rendered as numbered lines the model can address.
#[derive(Debug, Clone)]
pub struct DocumentView {
    /// Detected format.
    pub format: Format,
    /// `1| text` lines (docx), `Sheet!A1\tvalue` (xlsx), `slide.element| text` (pptx).
    pub lines: Vec<String>,
    /// True when `lines` was truncated to [`CONTEXT_CHARS`].
    pub truncated: bool,
    /// How the slice was chosen. Empty when the whole file fit.
    pub window: String,
}

impl DocumentView {
    /// Open `path` and render a view. `from` / `to` are 1-based inclusive,
    /// same as `bl docx view`, and win when set. Otherwise phrases from
    /// `instruction` select the window. `sheet` selects an xlsx sheet.
    pub fn open(
        path: &Path,
        from: Option<usize>,
        to: Option<usize>,
        sheet: Option<&str>,
        instruction: Option<&str>,
    ) -> Result<Self, AiError> {
        Ok(Self::windows(path, from, to, sheet, instruction)?
            .into_iter()
            .next()
            .expect("windows always returns at least one view"))
    }

    /// Same as [`open`], but a document-wide instruction (or a hit list
    /// that does not fit in one prompt) is split into chunks instead of
    /// dropping later paragraphs.
    pub fn windows(
        path: &Path,
        from: Option<usize>,
        to: Option<usize>,
        sheet: Option<&str>,
        instruction: Option<&str>,
    ) -> Result<Vec<Self>, AiError> {
        let format = Format::from_path(path)?;
        if !path.is_file() {
            return Err(AiError::missing(path.to_path_buf()));
        }
        let mut lines = match format {
            Format::Docx => view_docx(path)?,
            Format::Xlsx => view_xlsx(path, sheet)?,
            Format::Pptx => view_pptx(path)?,
        };
        let mut note = String::new();
        let mut walk_all = from.is_some() || to.is_some();
        if from.is_some() || to.is_some() {
            slice_lines(&mut lines, from, to);
            note = "explicit --from/--to".into();
        } else if let Some(instruction) = instruction {
            if is_global_instruction(instruction) {
                note = "document-wide instruction".into();
                walk_all = true;
            } else if let Some(w) = window_from_instruction(&mut lines, instruction) {
                note = w;
                walk_all = true;
            } else {
                note = "prefix — no phrase from the instruction was found".into();
            }
        }
        let mut views = pack_windows(format, lines, note);
        if !walk_all && views.len() > 1 {
            views.truncate(1);
        }
        Ok(views)
    }

    /// Characters that will actually go into the prompt (abbreviated).
    pub fn prompt_chars(&self) -> usize {
        self.lines
            .iter()
            .map(|line| abbreviate_line(line, PROMPT_LINE_CHARS).len() + 1)
            .sum()
    }

    /// Prompt block: format header plus numbered lines.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "# {} ({} lines; INDEX is the number before |, not 1..{})\n",
            self.format,
            self.lines.len(),
            self.lines.len()
        ));
        if !self.window.is_empty() {
            out.push_str(&format!("# window: {}\n", self.window));
        }
        for line in &self.lines {
            out.push_str(&abbreviate_line(line, PROMPT_LINE_CHARS));
            out.push('\n');
        }
        out
    }
}

fn view_docx(path: &Path) -> Result<Vec<String>, AiError> {
    let doc = Docx::open(path).map_err(|e| AiError::Apply(e.to_string()))?;
    doc.text_lines().map_err(|e| AiError::Apply(e.to_string()))
}

fn view_xlsx(path: &Path, sheet: Option<&str>) -> Result<Vec<String>, AiError> {
    let wb = Xlsx::open(path).map_err(|e| AiError::Apply(e.to_string()))?;
    let views = wb.view(sheet).map_err(|e| AiError::Apply(e.to_string()))?;
    let mut lines = Vec::new();
    for sheet_view in views {
        for row in sheet_view.rows {
            // `A1\tvalue` from blackline-xlsx. Prefix the sheet so the model
            // can fill `set_cell.sheet`.
            lines.push(format!("{}!{row}", sheet_view.sheet));
        }
    }
    Ok(lines)
}

fn view_pptx(path: &Path) -> Result<Vec<String>, AiError> {
    let deck = Pptx::open(path).map_err(|e| AiError::Apply(e.to_string()))?;
    let views = deck.view().map_err(|e| AiError::Apply(e.to_string()))?;
    let mut lines = Vec::new();
    for slide in views {
        for (i, text) in slide.elements.iter().enumerate() {
            lines.push(format!("{}.{}| {text}", slide.index, i + 1));
        }
    }
    Ok(lines)
}

fn pack_windows(format: Format, lines: Vec<String>, note: String) -> Vec<DocumentView> {
    if lines.is_empty() {
        return vec![DocumentView {
            format,
            lines,
            truncated: false,
            window: note,
        }];
    }
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut used = 0usize;
    for line in lines {
        let cost = abbreviate_line(&line, PROMPT_LINE_CHARS).len() + 1;
        if !cur.is_empty() && used.saturating_add(cost) > CONTEXT_CHARS {
            groups.push(std::mem::take(&mut cur));
            used = 0;
        }
        used = used.saturating_add(cost);
        cur.push(line);
    }
    if !cur.is_empty() {
        groups.push(cur);
    }
    let n = groups.len();
    groups
        .into_iter()
        .enumerate()
        .map(|(i, lines)| {
            let window = if n > 1 && !note.is_empty() {
                format!("{note}  chunk {}/{n}", i + 1)
            } else if n > 1 {
                format!("chunk {}/{n}", i + 1)
            } else {
                note.clone()
            };
            DocumentView {
                format,
                lines,
                truncated: n > 1,
                window,
            }
        })
        .collect()
}

/// `every paragraph` / `throughout the document` — walk the whole file.
pub(crate) fn is_global_instruction(instruction: &str) -> bool {
    let s = instruction.to_ascii_lowercase();
    let every = s.contains("every ")
        || s.contains("each ")
        || s.contains("all paragraph")
        || s.contains("all clause")
        || s.contains("all heading")
        || s.contains("throughout")
        || s.contains("whole document")
        || s.contains("entire document");
    every
        && (s.contains("paragraph")
            || s.contains("clause")
            || s.contains("heading")
            || s.contains("document")
            || s.contains("line")
            || s.contains("cell")
            || s.contains("row"))
}

fn abbreviate_line(line: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if line.chars().count() <= max {
        return line.to_string();
    }
    let mut out: String = line.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn slice_lines(lines: &mut Vec<String>, from: Option<usize>, to: Option<usize>) {
    if from.is_none() && to.is_none() {
        return;
    }
    let start = from.unwrap_or(1).saturating_sub(1);
    let end = to.unwrap_or(lines.len()).min(lines.len());
    if start >= lines.len() || start >= end {
        lines.clear();
        return;
    }
    *lines = lines[start..end].to_vec();
}

/// Keep hits for `instruction` plus [`HIT_PAD`] neighbors. Returns a note
/// when a window was applied.
fn window_from_instruction(lines: &mut Vec<String>, instruction: &str) -> Option<String> {
    if lines.is_empty() {
        return None;
    }
    let needles = needles_from_instruction(instruction);
    let mut hits = BTreeSet::new();
    if let Some(found) = hit_indices(lines, &needles) {
        hits.extend(found);
    }
    if is_structural_title(instruction) {
        hits.insert(0);
    }
    if hits.is_empty() {
        return None;
    }
    let hits: Vec<usize> = hits.into_iter().collect();
    let kept = expand_hits(&hits, lines.len(), HIT_PAD);
    let labels: Vec<String> = kept
        .iter()
        .filter(|i| hits.contains(i))
        .map(|&i| line_label(&lines[i]))
        .collect();
    *lines = kept.into_iter().map(|i| lines[i].clone()).collect();
    Some(format!(
        "instruction hits at {}",
        if labels.is_empty() {
            "?".into()
        } else {
            labels.join(", ")
        }
    ))
}

fn hit_indices(lines: &[String], needles: &[String]) -> Option<Vec<usize>> {
    let mut hits = BTreeSet::new();
    for needle in needles {
        if needle.chars().count() < 2 {
            continue;
        }
        let found: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line_matches(line, needle))
            .map(|(i, _)| i)
            .collect();
        if found.is_empty() || too_common(found.len(), lines.len()) {
            continue;
        }
        hits.extend(found);
    }
    if hits.is_empty() {
        None
    } else {
        Some(hits.into_iter().collect())
    }
}

fn too_common(hits: usize, n_lines: usize) -> bool {
    hits > 12 || (n_lines > 8 && hits * 4 > n_lines)
}

/// Phrase needles use substring match. Short single tokens are whole-word
/// so `title` does not hit `entitled`.
fn line_matches(line: &str, needle: &str) -> bool {
    let hay = line.to_lowercase();
    let n = needle.to_lowercase();
    if n.chars().any(char::is_whitespace) || n.chars().count() >= 12 {
        return hay.contains(&n);
    }
    hay.split(|c: char| !c.is_alphanumeric() && c != '-')
        .any(|word| word == n)
}

fn expand_hits(hits: &[usize], n_lines: usize, pad: usize) -> Vec<usize> {
    let last = n_lines.saturating_sub(1);
    let mut out = BTreeSet::new();
    for &i in hits {
        let lo = i.saturating_sub(pad);
        let hi = i.saturating_add(pad).min(last);
        for j in lo..=hi {
            out.insert(j);
        }
    }
    out.into_iter().collect()
}

fn line_label(line: &str) -> String {
    if let Some((prefix, _)) = line.split_once('|') {
        return prefix.trim().to_string();
    }
    if let Some((prefix, _)) = line.split_once('\t') {
        return prefix.trim().to_string();
    }
    line.chars().take(24).collect()
}

/// Phrases the file is searched for. Quoted spans and `change X to Y` /
/// `replace X with Y` pairs first; otherwise leftover content words.
pub(crate) fn needles_from_instruction(instruction: &str) -> Vec<String> {
    let mut out = Vec::new();
    push_quoted(&mut out, instruction);
    if let Some((old, new)) = change_pair(instruction) {
        if !is_structural_old(&old) {
            push_unique(&mut out, old);
        }
        if new.len() >= 3 {
            push_unique(&mut out, new);
        }
    }
    if out.is_empty() {
        for word in instruction.split(|c: char| !c.is_alphanumeric() && c != '-') {
            // Short leftover tokens (`every`, `first`, `have`) are not
            // content. They used to select random clauses.
            if word.len() >= 6 && !is_skip(word) {
                push_unique(&mut out, word.to_string());
            }
        }
    }
    out
}

fn push_quoted(out: &mut Vec<String>, instruction: &str) {
    for (open, close) in [('"', '"'), ('\u{201c}', '\u{201d}'), ('\'', '\'')] {
        let mut rest = instruction;
        while let Some(start) = rest.find(open) {
            let after = &rest[start + open.len_utf8()..];
            let Some(end) = after.find(close) else {
                break;
            };
            let inner = after[..end].trim();
            if inner.len() >= 2 {
                push_unique(out, inner.to_string());
            }
            rest = &after[end + close.len_utf8()..];
        }
    }
}

fn change_pair(instruction: &str) -> Option<(String, String)> {
    let lower = instruction.to_ascii_lowercase();
    for (verb, mid) in [
        ("change ", " to "),
        ("replace ", " with "),
        ("rename ", " to "),
        ("set ", " to "),
    ] {
        let Some(verb_at) = lower.find(verb) else {
            continue;
        };
        let after_verb = verb_at + verb.len();
        let rest_lower = &lower[after_verb..];
        let Some(mid_at) = rest_lower.find(mid) else {
            continue;
        };
        let old = trim_phrase(&instruction[after_verb..after_verb + mid_at]);
        let new = trim_phrase(&instruction[after_verb + mid_at + mid.len()..]);
        if old.len() >= 2 {
            return Some((old, new));
        }
    }
    None
}

fn trim_phrase(s: &str) -> String {
    s.trim()
        .trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'))
        .trim()
        .to_string()
}

fn is_structural_title(instruction: &str) -> bool {
    change_pair(instruction).is_some_and(|(old, _)| is_structural_old(&old))
}

fn is_structural_old(old: &str) -> bool {
    matches!(
        old.to_ascii_lowercase().as_str(),
        "title"
            | "the title"
            | "heading"
            | "the heading"
            | "header"
            | "the header"
            | "document title"
    )
}

fn is_skip(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "change"
            | "replace"
            | "rename"
            | "update"
            | "please"
            | "make"
            | "this"
            | "that"
            | "with"
            | "from"
            | "into"
            | "document"
            | "clause"
            | "paragraph"
            | "section"
            | "comment"
            | "insert"
            | "delete"
            | "flag"
            | "every"
            | "each"
            | "have"
            | "first"
            | "caps"
            | "capital"
            | "capitalize"
            | "uppercase"
            | "lowercase"
            | "entire"
            | "whole"
            | "throughout"
            | "paragraphs"
            | "clauses"
            | "headings"
            | "them"
            | "their"
            | "then"
            | "also"
            | "just"
            | "only"
            | "been"
            | "being"
            | "will"
            | "shall"
            | "must"
            | "here"
            | "there"
            | "about"
            | "would"
            | "could"
            | "should"
            | "these"
            | "those"
            | "after"
            | "before"
    )
}

fn push_unique(out: &mut Vec<String>, value: String) {
    let key = value.to_ascii_lowercase();
    if out.iter().any(|e| e.eq_ignore_ascii_case(&key)) {
        return;
    }
    out.push(value);
}

fn truncate_chars(lines: &mut Vec<String>, budget: usize) -> bool {
    let mut used: usize = 0;
    for (i, line) in lines.iter().enumerate() {
        used = used.saturating_add(line.len()).saturating_add(1);
        if used > budget {
            lines.truncate(i.max(1));
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        abbreviate_line, change_pair, expand_hits, hit_indices, is_global_instruction,
        needles_from_instruction, pack_windows, truncate_chars, window_from_instruction,
        DocumentView,
    };
    use crate::ai::format::Format;
    use blackline_docx::Docx;

    #[test]
    fn truncate_keeps_a_prefix() {
        let mut lines = vec!["aaaa".into(), "bbbb".into(), "cccc".into()];
        assert!(truncate_chars(&mut lines, 10));
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn change_title_needles() {
        let n = needles_from_instruction("change title to SAASY");
        assert!(n.iter().any(|s| s.eq_ignore_ascii_case("SAASY")), "{n:?}");
        assert!(
            !n.iter().any(|s| s.eq_ignore_ascii_case("title")),
            "title is the document start, not a search needle: {n:?}"
        );
    }

    #[test]
    fn change_thirty_days_needles() {
        let n = needles_from_instruction("change thirty days to sixty days");
        assert!(
            n.iter().any(|s| s.to_ascii_lowercase().contains("thirty")),
            "{n:?}"
        );
    }

    #[test]
    fn quoted_needle() {
        let n = needles_from_instruction("flag the \"Purchase Price\" definition");
        assert!(
            n.iter().any(|s| s.eq_ignore_ascii_case("Purchase Price")),
            "{n:?}"
        );
    }

    #[test]
    fn change_pair_splits() {
        let (old, new) = change_pair("Please change thirty days to sixty days.").unwrap();
        assert_eq!(old, "thirty days");
        assert_eq!(new, "sixty days");
    }

    #[test]
    fn window_keeps_the_hit_and_neighbor() {
        let mut lines = vec![
            "1| Preamble.".into(),
            "2| Recitals.".into(),
            "3| Notice is thirty days.".into(),
            "4| More recitals.".into(),
            "5| Signature.".into(),
        ];
        let note = window_from_instruction(&mut lines, "change thirty days to sixty days").unwrap();
        assert!(note.contains('3'), "{note}");
        assert!(lines.iter().any(|l| l.contains("thirty")));
        assert!(!lines.iter().any(|l| l.contains("Signature")));
        // original index is preserved so apply still hits paragraph 3
        assert!(lines.iter().any(|l| l.starts_with("3|")));
    }

    #[test]
    fn window_title_is_the_first_paragraph() {
        let mut lines = vec![
            "1| Y Combinator SaaS Agreement".into(),
            "2| Recitals.".into(),
            "3| Buyer is entitled to fees.".into(),
            "4| More recitals.".into(),
            "5| Signature.".into(),
        ];
        let note = window_from_instruction(&mut lines, "change title to SAASY").unwrap();
        assert!(note.contains('1'), "{note}");
        assert!(lines.iter().any(|l| l.contains("SaaS Agreement")));
        assert!(!lines.iter().any(|l| l.contains("Signature")));
        assert!(lines.iter().any(|l| l.starts_with("1|")));
        assert!(!lines.iter().any(|l| l.contains("entitled")));
    }

    #[test]
    fn title_does_not_match_entitled() {
        let lines = vec![
            "1| Buyer is entitled to fees.".into(),
            "2| Title insurance.".into(),
        ];
        let hits = hit_indices(&lines, &["title".into()]).unwrap();
        assert_eq!(hits, vec![1]);
    }

    #[test]
    fn common_words_do_not_select_the_whole_file() {
        let lines: Vec<String> = (1..=20).map(|i| format!("{i}| the party agrees")).collect();
        assert!(hit_indices(&lines, &["the".into()]).is_none());
    }

    #[test]
    fn expand_includes_neighbors() {
        assert_eq!(expand_hits(&[2], 6, 1), vec![1, 2, 3]);
    }

    #[test]
    fn open_docx_windows_from_instruction() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("c.docx");
        Docx::from_paragraphs(&[
            "Preamble.",
            "Recitals.",
            "Notice is thirty days.",
            "More recitals.",
            "Signature.",
        ])
        .unwrap()
        .save(&path)
        .unwrap();
        let view = DocumentView::open(
            &path,
            None,
            None,
            None,
            Some("change thirty days to sixty days"),
        )
        .unwrap();
        assert!(view.window.contains("instruction hits"), "{}", view.window);
        assert!(view.lines.iter().any(|l| l.contains("thirty")));
        assert!(!view.lines.iter().any(|l| l.contains("Signature")));
        assert!(view.lines.iter().any(|l| l.starts_with("3|")));
    }

    #[test]
    fn open_docx_title_is_first_paragraph() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("c.docx");
        Docx::from_paragraphs(&[
            "Y Combinator SaaS Agreement",
            "Recitals.",
            "Buyer is entitled to fees.",
            "More recitals.",
            "Signature.",
        ])
        .unwrap()
        .save(&path)
        .unwrap();
        let view =
            DocumentView::open(&path, None, None, None, Some("change title to SAASY")).unwrap();
        assert!(view.window.contains("instruction hits"), "{}", view.window);
        assert!(view.lines.iter().any(|l| l.starts_with("1|")));
        assert!(view.lines.iter().any(|l| l.contains("SaaS Agreement")));
        assert!(!view.lines.iter().any(|l| l.contains("Signature")));
    }

    #[test]
    fn every_paragraph_is_global_and_has_no_needles() {
        let inst = "update every paragraph to have all caps first";
        assert!(is_global_instruction(inst));
        assert!(
            needles_from_instruction(inst).is_empty(),
            "{:?}",
            needles_from_instruction(inst)
        );
        assert!(!is_global_instruction("change thirty days to sixty days"));
    }

    #[test]
    fn abbreviate_keeps_index_prefix() {
        let line = format!(
            "21| {}",
            "Customer shall own all right, title and interest. ".repeat(8)
        );
        let short = abbreviate_line(&line, 80);
        assert!(short.starts_with("21| "));
        assert!(short.ends_with('…'));
        assert!(short.chars().count() <= 80);
    }

    #[test]
    fn pack_windows_does_not_drop_later_indexes() {
        let lines: Vec<String> = (1..=24)
            .map(|i| format!("{i}| {}", "x".repeat(160)))
            .collect();
        let views = pack_windows(Format::Docx, lines, "document-wide instruction".into());
        assert!(views.len() >= 2, "{}", views.len());
        let last = views.last().unwrap();
        assert!(
            last.lines.iter().any(|l| l.starts_with("24|")),
            "{:?}",
            last.lines
        );
        assert!(last.window.contains("chunk"));
    }

    #[test]
    fn open_every_paragraph_walks_the_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("c.docx");
        let paras: Vec<String> = (1..=24)
            .map(|i| format!("Paragraph {i} {}", "word ".repeat(40)))
            .collect();
        let refs: Vec<&str> = paras.iter().map(String::as_str).collect();
        Docx::from_paragraphs(&refs).unwrap().save(&path).unwrap();
        let views = DocumentView::windows(
            &path,
            None,
            None,
            None,
            Some("update every paragraph to have all caps first"),
        )
        .unwrap();
        assert!(
            views[0].window.contains("document-wide"),
            "{}",
            views[0].window
        );
        let n: usize = views.iter().map(|v| v.lines.len()).sum();
        assert!(
            n >= 24,
            "{n} {:?}",
            views.iter().map(|v| v.lines.len()).collect::<Vec<_>>()
        );
        assert!(views
            .last()
            .unwrap()
            .lines
            .iter()
            .any(|l| l.contains("Paragraph 24")));
    }
}
