//! Numbered document view fed to the model. Same index space as `bl docx view`.
//!
//! The model still addresses 1-based indexes, but the user does not have to
//! pass `--from` / `--to`. When those flags are omitted, phrases from the
//! instruction are searched in the full file and only those hits (plus a
//! neighbor) go into the prompt.

use std::collections::BTreeSet;
use std::path::Path;

use blackline_docx::Docx;
use blackline_pptx::Pptx;
use blackline_xlsx::Xlsx;

use super::error::AiError;
use super::format::Format;

/// Soft cap on characters stuffed into the prompt. TinyLlama is 2k tokens
/// and Phi-3 is 4k. A long file is windowed to instruction hits first;
/// `--from` / `--to` is the explicit override.
pub const CONTEXT_CHARS: usize = 2_500;

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
        let format = Format::from_path(path)?;
        if !path.is_file() {
            return Err(AiError::missing(path.to_path_buf()));
        }
        let mut lines = match format {
            Format::Docx => view_docx(path)?,
            Format::Xlsx => view_xlsx(path, sheet)?,
            Format::Pptx => view_pptx(path)?,
        };
        let mut window = String::new();
        if from.is_some() || to.is_some() {
            slice_lines(&mut lines, from, to);
            window = "explicit --from/--to".into();
        } else if let Some(instruction) = instruction {
            if let Some(note) = window_from_instruction(&mut lines, instruction) {
                window = note;
            }
        }
        let truncated = truncate_chars(&mut lines, CONTEXT_CHARS);
        if truncated && window.is_empty() {
            window = "prefix — no phrase from the instruction was found".into();
        } else if truncated && window == "explicit --from/--to" {
            window = "explicit --from/--to, still over the prompt budget".into();
        }
        Ok(Self {
            format,
            lines,
            truncated,
            window,
        })
    }

    /// Prompt block: format header plus numbered lines.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {} ({} lines)\n", self.format, self.lines.len()));
        if !self.window.is_empty() {
            out.push_str(&format!("# window: {}\n", self.window));
        }
        for line in &self.lines {
            out.push_str(line);
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
            if word.len() >= 4 && !is_skip(word) {
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
        change_pair, expand_hits, hit_indices, needles_from_instruction, truncate_chars,
        window_from_instruction, DocumentView,
    };
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
}
