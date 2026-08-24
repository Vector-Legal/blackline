//! Apply a [`Plan`] through blackline. The LLM never touches the package.

use std::path::{Path, PathBuf};

use blackline_docx::{Docx, EditOp as DocxEditOp, Granularity, TrackOp};
use blackline_pptx::{EditOp as PptxEditOp, Pptx};
use blackline_xlsx::{EditOp as XlsxEditOp, Xlsx};
use serde::Serialize;

use crate::error::LlmError;
use crate::format::Format;
use crate::plan::{Op, Plan};

/// How a [`Plan`] is applied.
#[derive(Debug, Clone)]
pub struct ApplyOptions {
    /// Default author for tracked changes / comments.
    pub author: Option<String>,
    /// Word redline granularity.
    pub granularity: Granularity,
    /// DOCX: silent `edit` instead of `track`.
    pub no_track: bool,
    /// Best-effort.
    pub lenient: bool,
    /// Resolve without writing.
    pub dry_run: bool,
}

impl Default for ApplyOptions {
    fn default() -> Self {
        Self {
            author: None,
            granularity: Granularity::Word,
            no_track: false,
            lenient: false,
            dry_run: false,
        }
    }
}

/// Per-op row in the JSON report.
#[derive(Debug, Clone, Serialize)]
pub struct OpStat {
    /// Zero-based batch index.
    pub index: usize,
    /// Op name.
    pub op: String,
    /// `applied` or `failed`.
    pub status: &'static str,
    /// Detail.
    pub detail: String,
}

/// Result of applying a plan.
#[derive(Debug, Clone, Serialize)]
pub struct ApplyReport {
    /// Applied count.
    pub applied: usize,
    /// Failed count.
    pub failed: usize,
    /// `strict`, `lenient`, or `dry-run`.
    pub mode: &'static str,
    /// Per-op rows.
    pub ops: Vec<OpStat>,
}

/// Apply `plan` to `input`. When `output` is `Some`, write there (unless dry-run).
pub fn apply(
    input: &Path,
    output: Option<&Path>,
    plan: &Plan,
    opts: &ApplyOptions,
) -> Result<ApplyReport, LlmError> {
    let format = Format::from_path(input)?;
    for op in &plan.ops {
        op.require_format(format)?;
    }
    match format {
        Format::Docx => apply_docx(input, output, plan, opts),
        Format::Xlsx => apply_xlsx(input, output, plan, opts),
        Format::Pptx => apply_pptx(input, output, plan, opts),
    }
}

fn apply_docx(
    input: &Path,
    output: Option<&Path>,
    plan: &Plan,
    opts: &ApplyOptions,
) -> Result<ApplyReport, LlmError> {
    let doc = Docx::open(input).map_err(|e| LlmError::Apply(e.to_string()))?;
    if opts.no_track {
        let ops = plan
            .ops
            .iter()
            .map(to_docx_edit)
            .collect::<Result<Vec<_>, _>>()?;
        let mut builder = doc.edit(ops);
        if let Some(author) = &opts.author {
            builder = builder.author(author);
        }
        builder = builder.granularity(opts.granularity);
        if opts.lenient {
            builder = builder.lenient();
        }
        if opts.dry_run {
            builder = builder.dry_run();
        }
        let outcome = builder
            .apply()
            .map_err(|e| LlmError::Apply(e.to_string()))?;
        let report = map_docx_edit(&outcome.report);
        if let (Some(path), Some(edited)) = (output, outcome.document) {
            atomic_save(path, |p| {
                edited.save(p).map_err(|e| LlmError::Apply(e.to_string()))
            })?;
        }
        return Ok(report);
    }

    let author = opts.author.as_deref().ok_or_else(|| {
        LlmError::usage(
            "author required: tracked changes and comments must carry an explicit author \
             (pass --author or set BLACKLINE_AUTHOR)"
                .to_string(),
        )
    })?;
    let ops = plan
        .ops
        .iter()
        .map(to_track)
        .collect::<Result<Vec<_>, _>>()?;
    let mut builder = doc.track(ops).author(author).granularity(opts.granularity);
    if opts.lenient {
        builder = builder.lenient();
    }
    if opts.dry_run {
        builder = builder.dry_run();
    }
    let outcome = builder
        .apply()
        .map_err(|e| LlmError::Apply(e.to_string()))?;
    let report = map_track(&outcome.report);
    if let (Some(path), Some(edited)) = (output, outcome.document) {
        atomic_save(path, |p| {
            edited.save(p).map_err(|e| LlmError::Apply(e.to_string()))
        })?;
        let original = Docx::open(input).map_err(|e| LlmError::Apply(e.to_string()))?;
        let saved = Docx::open(path).map_err(|e| LlmError::Apply(e.to_string()))?;
        let health = saved
            .check_against(&original)
            .map_err(|e| LlmError::Apply(e.to_string()))?;
        if !health.passed() {
            return Err(LlmError::Apply(format!(
                "package check failed after apply: {}",
                serde_json::to_string(&health).unwrap_or_default()
            )));
        }
    }
    Ok(report)
}

fn apply_xlsx(
    input: &Path,
    output: Option<&Path>,
    plan: &Plan,
    opts: &ApplyOptions,
) -> Result<ApplyReport, LlmError> {
    let mut wb = Xlsx::open(input).map_err(|e| LlmError::Apply(e.to_string()))?;
    let ops = plan
        .ops
        .iter()
        .map(to_xlsx)
        .collect::<Result<Vec<_>, _>>()?;
    let edit_opts = blackline_xlsx::EditOptions {
        lenient: opts.lenient,
        dry_run: opts.dry_run,
    };
    let report = wb
        .edit(&ops, &edit_opts)
        .map_err(|e| LlmError::Apply(e.to_string()))?;
    if let Some(path) = output {
        if !opts.dry_run {
            atomic_save(path, |p| {
                wb.save(p).map_err(|e| LlmError::Apply(e.to_string()))
            })?;
        }
    }
    Ok(map_xlsx(&report))
}

fn apply_pptx(
    input: &Path,
    output: Option<&Path>,
    plan: &Plan,
    opts: &ApplyOptions,
) -> Result<ApplyReport, LlmError> {
    let mut deck = Pptx::open(input).map_err(|e| LlmError::Apply(e.to_string()))?;
    let ops = plan
        .ops
        .iter()
        .map(to_pptx)
        .collect::<Result<Vec<_>, _>>()?;
    let edit_opts = blackline_pptx::EditOptions {
        lenient: opts.lenient,
        dry_run: opts.dry_run,
    };
    let report = deck
        .edit(&ops, &edit_opts)
        .map_err(|e| LlmError::Apply(e.to_string()))?;
    if let Some(path) = output {
        if !opts.dry_run {
            atomic_save(path, |p| {
                deck.save(p).map_err(|e| LlmError::Apply(e.to_string()))
            })?;
        }
    }
    Ok(map_pptx(&report))
}

fn to_track(op: &Op) -> Result<TrackOp, LlmError> {
    Ok(match op {
        Op::Replace { index, old, new } => TrackOp::Replace {
            index: Some(require_index(*index)?),
            content_match: None,
            old: Some(old.clone()),
            new: new.clone(),
            author: None,
            date: None,
        },
        Op::Insert {
            index,
            position,
            text,
        } => TrackOp::Insert {
            index: Some(require_index(*index)?),
            content_match: None,
            position: position.as_str().to_string(),
            text: text.clone(),
            author: None,
            date: None,
        },
        Op::InsertParagraph {
            index,
            position,
            text,
        } => TrackOp::InsertParagraph {
            index: Some(require_index(*index)?),
            content_match: None,
            position: paragraph_position(*position).to_string(),
            text: text.clone(),
            author: None,
            date: None,
        },
        Op::Delete { index, text } => TrackOp::Delete {
            index: Some(require_index(*index)?),
            content_match: None,
            text: nonempty(text),
            old: None,
            author: None,
            date: None,
        },
        Op::Comment {
            index,
            anchor,
            text,
        } => TrackOp::Comment {
            index: Some(require_index(*index)?),
            content_match: None,
            anchor: nonempty(anchor),
            text: text.clone(),
            author: None,
            date: None,
        },
        other => {
            return Err(LlmError::Apply(format!(
                "op {} is not a Word track op",
                other.name()
            )));
        }
    })
}

fn to_docx_edit(op: &Op) -> Result<DocxEditOp, LlmError> {
    Ok(match op {
        Op::Replace { index, old, new } => DocxEditOp::Replace {
            index: Some(require_index(*index)?),
            old: old.clone(),
            new: new.clone(),
            content_match: None,
        },
        Op::Insert {
            index,
            position,
            text,
        }
        | Op::InsertParagraph {
            index,
            position,
            text,
        } => DocxEditOp::Insert {
            index: Some(require_index(*index)?),
            position: paragraph_position(*position).to_string(),
            content_match: None,
            text: Some(text.clone()),
            content: None,
            style: None,
            para_props: blackline_docx::ParaProps::default(),
        },
        Op::Delete { index, text } => {
            if text.is_empty() {
                DocxEditOp::Delete {
                    index: Some(require_index(*index)?),
                    range: None,
                    content_match: None,
                }
            } else {
                DocxEditOp::DeleteRun {
                    index: Some(require_index(*index)?),
                    content_match: None,
                    text: text.clone(),
                }
            }
        }
        Op::Comment {
            index,
            anchor,
            text,
        } => DocxEditOp::InsertComment {
            index: Some(require_index(*index)?),
            content_match: None,
            anchor: nonempty(anchor),
            text: text.clone(),
            author: None,
        },
        other => {
            return Err(LlmError::Apply(format!(
                "op {} is not a Word edit op",
                other.name()
            )));
        }
    })
}

fn to_xlsx(op: &Op) -> Result<XlsxEditOp, LlmError> {
    match op {
        Op::SetCell { sheet, cell, value } => Ok(XlsxEditOp::SetCell {
            sheet: sheet.clone(),
            cell: cell.clone(),
            value: Some(cell_value(value)),
            formula: None,
        }),
        other => Err(LlmError::Apply(format!(
            "op {} is not an Excel op",
            other.name()
        ))),
    }
}

fn to_pptx(op: &Op) -> Result<PptxEditOp, LlmError> {
    match op {
        Op::SetText {
            slide,
            element,
            text,
        } => Ok(PptxEditOp::SetText {
            slide: require_index(*slide)?,
            element: require_index(*element)?,
            text: text.clone(),
        }),
        other => Err(LlmError::Apply(format!(
            "op {} is not a PowerPoint op",
            other.name()
        ))),
    }
}

fn cell_value(raw: &str) -> serde_json::Value {
    let t = raw.trim();
    if t.starts_with('=') {
        return serde_json::Value::String(t.to_string());
    }
    if let Ok(n) = t.parse::<i64>() {
        return serde_json::json!(n);
    }
    if let Ok(n) = t.parse::<f64>() {
        return serde_json::json!(n);
    }
    if t.eq_ignore_ascii_case("true") {
        return serde_json::Value::Bool(true);
    }
    if t.eq_ignore_ascii_case("false") {
        return serde_json::Value::Bool(false);
    }
    serde_json::Value::String(raw.to_string())
}

fn require_index(index: u32) -> Result<usize, LlmError> {
    if index == 0 {
        return Err(LlmError::Apply(
            "indexes are 1-based; the model emitted 0".into(),
        ));
    }
    Ok(index as usize)
}

fn paragraph_position(position: crate::plan::Position) -> &'static str {
    match position {
        crate::plan::Position::Before | crate::plan::Position::Start => "before",
        crate::plan::Position::After | crate::plan::Position::End => "after",
    }
}

fn nonempty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn map_track(r: &blackline_docx::TrackReport) -> ApplyReport {
    ApplyReport {
        applied: r.applied,
        failed: r.failed,
        mode: r.mode,
        ops: r
            .ops
            .iter()
            .map(|o| OpStat {
                index: o.index,
                op: o.op.clone(),
                status: o.status,
                detail: o.detail.clone(),
            })
            .collect(),
    }
}

fn map_docx_edit(r: &blackline_docx::EditReport) -> ApplyReport {
    ApplyReport {
        applied: r.applied,
        failed: r.failed,
        mode: r.mode,
        ops: r
            .ops
            .iter()
            .map(|o| OpStat {
                index: o.index,
                op: o.op.clone(),
                status: o.status,
                detail: o.detail.clone(),
            })
            .collect(),
    }
}

fn map_xlsx(r: &blackline_xlsx::EditReport) -> ApplyReport {
    // `OpReport` is crate-private in blackline-xlsx; go through JSON.
    map_via_json(r.applied, r.failed, r.mode, r)
}

fn map_via_json(
    applied: usize,
    failed: usize,
    mode: &'static str,
    report: &impl Serialize,
) -> ApplyReport {
    let value = serde_json::to_value(report).unwrap_or_default();
    let ops = value
        .get("ops")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .map(|row| OpStat {
                    index: row
                        .get("index")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0) as usize,
                    op: row
                        .get("op")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("op")
                        .to_string(),
                    status: if row.get("status").and_then(serde_json::Value::as_str)
                        == Some("failed")
                    {
                        "failed"
                    } else {
                        "applied"
                    },
                    detail: row
                        .get("detail")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    ApplyReport {
        applied,
        failed,
        mode,
        ops,
    }
}

fn map_pptx(r: &blackline_pptx::EditReport) -> ApplyReport {
    ApplyReport {
        applied: r.applied,
        failed: r.failed,
        mode: r.mode,
        ops: r
            .ops
            .iter()
            .map(|o| OpStat {
                index: o.index,
                op: o.op.clone(),
                status: o.status,
                detail: o.detail.clone(),
            })
            .collect(),
    }
}

fn atomic_save(
    path: &Path,
    write: impl FnOnce(&Path) -> Result<(), LlmError>,
) -> Result<(), LlmError> {
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| LlmError::io_path(dir, e))?;
    }
    let tmp = path.with_file_name(format!(
        ".{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("out")
    ));
    write(&tmp)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        LlmError::Io(format!("rename {}: {e}", path.display()))
    })?;
    Ok(())
}

/// Resolve `-o` / `--in-place`. Dry-run may omit both.
pub fn resolve_output(
    input: &Path,
    output: Option<&Path>,
    in_place: bool,
    dry_run: bool,
) -> Result<Option<PathBuf>, LlmError> {
    match (output, in_place, dry_run) {
        (Some(_), true, _) => Err(LlmError::usage(
            "use either -o/--output or --in-place, not both".to_string(),
        )),
        (None, false, true) => Ok(None),
        (None, false, false) => Err(LlmError::usage(
            "pass -o/--output PATH or --in-place".to_string(),
        )),
        (Some(p), false, _) => Ok(Some(p.to_path_buf())),
        (None, true, _) => Ok(Some(input.to_path_buf())),
    }
}
