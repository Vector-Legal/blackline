//! Numbered document view fed to the model. Same index space as `bl docx view`.

use std::path::Path;

use blackline_docx::Docx;
use blackline_pptx::Pptx;
use blackline_xlsx::Xlsx;

use super::error::AiError;
use super::format::Format;

/// Soft cap on characters stuffed into the prompt. `--from` / `--to` is the
/// way to window a long file; this only stops a 200-page dump.
pub const CONTEXT_CHARS: usize = 16_384;

/// A file, opened and rendered as numbered lines the model can address.
#[derive(Debug, Clone)]
pub struct DocumentView {
    /// Detected format.
    pub format: Format,
    /// `1| text` lines (docx), `Sheet!A1\tvalue` (xlsx), `slide.element| text` (pptx).
    pub lines: Vec<String>,
    /// True when `lines` was truncated to [`CONTEXT_CHARS`].
    pub truncated: bool,
}

impl DocumentView {
    /// Open `path` and render a view. `from` / `to` are 1-based inclusive,
    /// same as `bl docx view`. `sheet` selects an xlsx sheet.
    pub fn open(
        path: &Path,
        from: Option<usize>,
        to: Option<usize>,
        sheet: Option<&str>,
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
        slice_lines(&mut lines, from, to);
        let truncated = truncate_chars(&mut lines, CONTEXT_CHARS);
        Ok(Self {
            format,
            lines,
            truncated,
        })
    }

    /// Prompt block: format header plus numbered lines.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {} ({} lines)\n", self.format, self.lines.len()));
        if self.truncated {
            out.push_str("# view truncated; pass --from/--to to window the file\n");
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
    use super::truncate_chars;

    #[test]
    fn truncate_keeps_a_prefix() {
        let mut lines = vec!["aaaa".into(), "bbbb".into(), "cccc".into()];
        assert!(truncate_chars(&mut lines, 10));
        assert_eq!(lines.len(), 2);
    }
}
