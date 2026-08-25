//! The only types the model is allowed to emit. Applied through blackline
//! `TrackOp` / `EditOp` — the model never writes OOXML itself.

use serde::{Deserialize, Serialize};

use super::error::AiError;
use super::format::Format;

// The Parse/Schema derives expand to `kalosm_sample::…`. Keep the crate
// linked when the feature is on (Cargo.toml: kalosm = ["dep:kalosm-sample"]).
#[cfg(feature = "kalosm")]
use kalosm_sample as _;

/// A batch of ops. Constrained generation (Kalosm `Parse`) targets this type
/// so the model cannot drift into free prose.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "kalosm",
    derive(kalosm::language::Parse, kalosm::language::Schema)
)]
pub struct Plan {
    /// Ops in apply order. Empty means the instruction needs no edit.
    #[serde(default)]
    pub ops: Vec<Op>,
}

/// Insert position relative to a numbered paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(
    feature = "kalosm",
    derive(kalosm::language::Parse, kalosm::language::Schema)
)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    /// Before the anchored paragraph / span.
    Before,
    /// After the anchored paragraph / span.
    #[default]
    After,
    /// Start of the paragraph.
    Start,
    /// End of the paragraph.
    End,
}

impl Position {
    /// Track / edit JSON value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
            Self::Start => "start",
            Self::End => "end",
        }
    }
}

/// One edit. The vocabulary is deliberately smaller than blackline's full
/// op set: replace / insert / delete / comment for Word, `set_cell` for
/// Excel, `set_text` for PowerPoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "kalosm",
    derive(kalosm::language::Parse, kalosm::language::Schema)
)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Replace `old` with `new` in a numbered paragraph. `old` must be copied
    /// from the view, not paraphrased.
    Replace {
        /// 1-based view index (`bl docx view`).
        index: u32,
        /// Exact span to replace.
        old: String,
        /// Replacement text.
        new: String,
    },
    /// Insert tracked text next to a paragraph.
    Insert {
        /// 1-based view index.
        index: u32,
        /// Where to put `text`.
        #[serde(default)]
        position: Position,
        /// Inserted text.
        text: String,
    },
    /// Insert a whole paragraph as a tracked insertion.
    InsertParagraph {
        /// Anchor view index.
        index: u32,
        /// `before` or `after` the anchor.
        #[serde(default)]
        position: Position,
        /// Paragraph text.
        text: String,
    },
    /// Delete a span (or the whole paragraph when `text` is empty).
    Delete {
        /// 1-based view index.
        index: u32,
        /// Span to delete. Empty means the whole visible paragraph.
        #[serde(default)]
        text: String,
    },
    /// Word comment anchored on `anchor` text.
    Comment {
        /// 1-based view index.
        index: u32,
        /// Text the comment range wraps. Empty = whole paragraph.
        #[serde(default)]
        anchor: String,
        /// Comment body.
        text: String,
    },
    /// Set one spreadsheet cell.
    SetCell {
        /// Sheet name or 1-based index.
        sheet: String,
        /// A1 reference.
        cell: String,
        /// Cell value (string, number, `=FORMULA`).
        value: String,
    },
    /// Replace a PowerPoint text frame.
    SetText {
        /// 1-based slide.
        slide: u32,
        /// 1-based text element.
        element: u32,
        /// New text.
        text: String,
    },
}

impl Op {
    /// Stable name, same as the serde tag.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Replace { .. } => "replace",
            Self::Insert { .. } => "insert",
            Self::InsertParagraph { .. } => "insert_paragraph",
            Self::Delete { .. } => "delete",
            Self::Comment { .. } => "comment",
            Self::SetCell { .. } => "set_cell",
            Self::SetText { .. } => "set_text",
        }
    }

    /// Format this op belongs to.
    pub fn format(&self) -> Format {
        match self {
            Self::Replace { .. }
            | Self::Insert { .. }
            | Self::InsertParagraph { .. }
            | Self::Delete { .. }
            | Self::Comment { .. } => Format::Docx,
            Self::SetCell { .. } => Format::Xlsx,
            Self::SetText { .. } => Format::Pptx,
        }
    }

    pub(crate) fn require_format(&self, expected: Format) -> Result<(), AiError> {
        let got = self.format();
        if got == expected {
            Ok(())
        } else {
            Err(AiError::Apply(format!(
                "op {} is for {got}, but the file is {expected}",
                self.name()
            )))
        }
    }
}

/// Something that turns a numbered view + instruction into a [`Plan`].
///
/// The Kalosm backend is the production implementation. Tests inject a
/// [`StaticCompleter`]. This is the only abstraction in the crate.
pub trait Completer: Send + Sync {
    /// Produce ops. Must not apply them.
    fn complete(
        &self,
        view: &super::view::DocumentView,
        instruction: &str,
    ) -> impl std::future::Future<Output = Result<Plan, AiError>> + Send;
}

/// Completer that returns a canned plan. Tests inject this so the pipeline
/// can be exercised without downloading a model.
#[derive(Debug, Clone)]
pub struct StaticCompleter {
    plan: Plan,
}

impl StaticCompleter {
    /// Wrap a plan.
    pub fn new(plan: Plan) -> Self {
        Self { plan }
    }
}

impl Completer for StaticCompleter {
    async fn complete(
        &self,
        _view: &super::view::DocumentView,
        _instruction: &str,
    ) -> Result<Plan, AiError> {
        Ok(self.plan.clone())
    }
}

/// System prompt. The view and instruction are the user message.
pub fn system_prompt(format: Format) -> &'static str {
    match format {
        Format::Docx => {
            "You edit a Word document by emitting operations against the numbered view. \
             Each line is `INDEX| text`. Put INDEX in `index` — it is the number before |, \
             not 1..N of this window. Copy a short `old`/`anchor` (a few words from the \
             visible text). Never paste a whole paragraph, never copy an ellipsis (…), \
             and never join several INDEX lines into one `old`. \
             For capitalization, replace only the first word. Prefer replace. \
             Never insert when existing words should change — emit replace. \
             DOCX ops become Word tracked changes. Empty ops list if nothing must change."
        }
        Format::Xlsx => {
            "You edit an Excel workbook by emitting set_cell operations. \
             Each view line is `Sheet!A1<TAB>value`. Copy sheet names and A1 refs exactly. \
             Only emit ops the instruction requires. Empty ops list if nothing must change."
        }
        Format::Pptx => {
            "You edit a PowerPoint deck by emitting set_text operations. \
             Each view line is `slide.element| text`. Slide and element are 1-based. \
             Only emit ops the instruction requires. Empty ops list if nothing must change."
        }
    }
}

/// User message: view + instruction.
pub fn user_prompt(view: &super::view::DocumentView, instruction: &str) -> String {
    format!("{}\n# Instruction\n{}", view.render(), instruction.trim())
}

/// Extra line when the model is not token-constrained (Metal).
pub fn json_output_instruction() -> &'static str {
    " Reply with one JSON object {\"ops\":[...]} and nothing else. \
     index is the number before | on the line. old/new are a few words from that one line. \
     Do not join lines with |. \
     Example: {\"ops\":[{\"op\":\"replace\",\"index\":21,\"old\":\"Customer\",\"new\":\"CUSTOMER\"}]} \
     or {\"ops\":[]}."
}

/// True when `text` contains a fully closed top-level JSON object.
pub(crate) fn json_object_complete(text: &str) -> bool {
    extract_json_object(text).is_some()
}

/// New tokens for a plan. Also the hard stop; generation should stop
/// earlier once [`json_object_complete`] is true.
pub(crate) const PLAN_MAX_TOKENS: u32 = 1536;

/// One in-place progress line for a multi-chunk plan.
pub(crate) fn eprint_chunk_progress(done: usize, total: usize) {
    if total <= 1 {
        return;
    }
    use std::io::Write;
    eprint!("\rplanning {done}/{total}");
    let _ = std::io::stderr().flush();
    if done == total {
        eprintln!();
    }
}

/// Parse a [`Plan`] from model text. Accepts a bare object or one wrapped
/// in prose / a markdown fence.
pub fn parse_plan_json(text: &str) -> Result<Plan, AiError> {
    let trimmed = text.trim();
    if let Ok(plan) = serde_json::from_str::<Plan>(trimmed) {
        return Ok(plan);
    }
    if let Some(json) = extract_json_object(trimmed) {
        if let Ok(plan) = serde_json::from_str::<Plan>(json) {
            return Ok(plan);
        }
    }
    if let Some(plan) = salvage_plan(trimmed) {
        return Ok(plan);
    }
    if trimmed.contains('{') {
        return Err(AiError::Model(format!(
            "model cut off mid-JSON (generation cap). \
             Keep `old`/`new` to a few words, not the whole paragraph. got: {}",
            truncate_for_error(trimmed)
        )));
    }
    Err(AiError::Model(format!(
        "model did not emit a plan object. got: {}",
        truncate_for_error(trimmed)
    )))
}

/// Keep complete ops when generation stops in the middle of the next one.
fn salvage_plan(text: &str) -> Option<Plan> {
    let ops_key = text.find("\"ops\"")?;
    let after_key = &text[ops_key + 5..];
    let bracket = after_key.find('[')?;
    let body = &after_key[bracket + 1..];
    let bytes = body.as_bytes();
    let mut i = 0usize;
    let mut ops = Vec::new();
    loop {
        while i < body.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if i >= body.len() || bytes[i] == b']' {
            break;
        }
        if bytes[i] != b'{' {
            break;
        }
        let Some(obj) = extract_json_object(&body[i..]) else {
            break;
        };
        match serde_json::from_str::<Op>(obj) {
            Ok(op) => {
                ops.push(op);
                i += obj.len();
            }
            Err(_) => break,
        }
    }
    if ops.is_empty() {
        None
    } else {
        Some(Plan { ops })
    }
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn truncate_for_error(text: &str) -> String {
    const MAX: usize = 240;
    if text.len() <= MAX {
        return text.to_string();
    }
    format!("{}…", &text[..MAX])
}

#[cfg(test)]
mod tests {
    use super::{system_prompt, user_prompt, Op, Plan};

    #[test]
    fn plan_round_trips_json() {
        let plan = Plan {
            ops: vec![Op::Replace {
                index: 1,
                old: "thirty".into(),
                new: "sixty".into(),
            }],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let back: Plan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ops.len(), 1);
        assert_eq!(back.ops[0].name(), "replace");
    }

    #[test]
    fn prompts_name_the_format() {
        use crate::ai::format::Format;
        assert!(system_prompt(Format::Docx).contains("tracked"));
        assert!(system_prompt(Format::Xlsx).contains("set_cell"));
        assert!(system_prompt(Format::Pptx).contains("set_text"));
        let view = crate::ai::view::DocumentView {
            format: Format::Docx,
            lines: vec!["1| hello".into()],
            truncated: false,
            window: String::new(),
        };
        let user = user_prompt(&view, "change hello");
        assert!(user.contains("hello"));
        assert!(user.contains("change hello"));
        assert!(user.contains("INDEX"));
    }

    #[test]
    fn parse_plan_from_fence_and_prose() {
        let plan = super::parse_plan_json(
            "Sure.\n```json\n{\"ops\":[{\"op\":\"replace\",\"index\":1,\"old\":\"a\",\"new\":\"b\"}]}\n```\n",
        )
        .unwrap();
        assert_eq!(plan.ops.len(), 1);
        assert_eq!(plan.ops[0].name(), "replace");
        let empty = super::parse_plan_json("here you go {\"ops\":[]} thanks").unwrap();
        assert!(empty.ops.is_empty());
    }

    #[test]
    fn salvage_keeps_complete_ops_from_truncated_json() {
        let plan = super::parse_plan_json(
            r#"{"ops":[
{"op":"replace","index":1,"old":"Customer:","new":"CUSTOMER:"},
{"op":"replace","index":2,"old":"Contact:","new":"CONTACT:"},
{"op":"repl"#,
        )
        .unwrap();
        assert_eq!(plan.ops.len(), 2);
        assert_eq!(plan.ops[0].name(), "replace");
    }

    #[test]
    fn parse_plan_rejects_garbage() {
        let err = super::parse_plan_json("I cannot do that.").unwrap_err();
        assert!(err.to_string().contains("did not emit a plan"));
        let cut = super::parse_plan_json(
            r#"{"ops":[{"op":"replace","index":1,"old":"H4: Customer shall own"#,
        )
        .unwrap_err();
        assert!(cut.to_string().contains("mid-JSON"), "{cut}");
        assert!(super::json_object_complete(r#"{"ops":[]}"#));
        assert!(!super::json_object_complete(r#"{"ops":[{"op":"replace""#));
    }
}
