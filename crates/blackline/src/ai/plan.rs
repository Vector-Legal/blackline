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
             Copy `old` / `anchor` / `text` spans exactly from the view. Indexes are 1-based. \
             Only emit ops the instruction requires. Prefer replace over delete+insert. \
             DOCX ops become Word tracked changes a lawyer can accept or reject. \
             Do not invent indexes. If nothing must change, emit an empty ops list."
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
     Example: {\"ops\":[{\"op\":\"replace\",\"index\":1,\"old\":\"Hello\",\"new\":\"Hi\"}]} \
     or {\"ops\":[]}."
}

/// Parse a [`Plan`] from model text. Accepts a bare object or one wrapped
/// in prose / a markdown fence.
pub fn parse_plan_json(text: &str) -> Result<Plan, AiError> {
    let trimmed = text.trim();
    if let Ok(plan) = serde_json::from_str::<Plan>(trimmed) {
        return Ok(plan);
    }
    let Some(json) = extract_json_object(trimmed) else {
        return Err(AiError::Model(format!(
            "model did not emit a plan object. got: {}",
            truncate_for_error(trimmed)
        )));
    };
    serde_json::from_str::<Plan>(json).map_err(|e| {
        AiError::Model(format!(
            "model emitted JSON that is not a plan ({e}). got: {}",
            truncate_for_error(json)
        ))
    })
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
        };
        let user = user_prompt(&view, "change hello");
        assert!(user.contains("hello"));
        assert!(user.contains("change hello"));
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
    fn parse_plan_rejects_garbage() {
        let err = super::parse_plan_json("I cannot do that.").unwrap_err();
        assert!(err.to_string().contains("did not emit a plan"));
    }
}
