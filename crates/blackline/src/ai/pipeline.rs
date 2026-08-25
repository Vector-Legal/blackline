//! Pipeline tests: a canned completer drives blackline. No model download.

use super::apply::ApplyOptions;
use super::cli::run_with_completer;
use super::plan::{Op, Plan, Position, StaticCompleter};
use blackline_docx::Docx;
use blackline_pptx::{CreateSpec, Pptx, SlideSpec};
use blackline_xlsx::Xlsx;
use tempfile::TempDir;

fn dir() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
async fn docx_replace_is_a_tracked_redline() {
    let dir = dir();
    let input = dir.path().join("in.docx");
    let output = dir.path().join("out.docx");
    Docx::from_paragraphs(&["The notice period is thirty (30) days."])
        .unwrap()
        .save(&input)
        .unwrap();

    let plan = Plan {
        ops: vec![Op::Replace {
            index: 1,
            old: "thirty (30)".into(),
            new: "sixty (60)".into(),
        }],
    };
    let report = run_with_completer(
        &input,
        "change thirty days to sixty",
        Some(&output),
        ApplyOptions {
            author: Some("Jane Doe".into()),
            ..ApplyOptions::default()
        },
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap();
    assert_eq!(report.apply.failed, 0);
    assert_eq!(report.apply.applied, 1);

    let original = Docx::open(&input).unwrap();
    let edited = Docx::open(&output).unwrap();
    assert!(edited.check_against(&original).unwrap().passed());
    assert!(edited.visible_text().unwrap().contains("sixty (60)"));
}

#[tokio::test]
async fn docx_reject_all_restores_original() {
    let dir = dir();
    let input = dir.path().join("in.docx");
    let output = dir.path().join("out.docx");
    let original_text = "Pay within thirty days.";
    Docx::from_paragraphs(&[original_text])
        .unwrap()
        .save(&input)
        .unwrap();

    let plan = Plan {
        ops: vec![Op::Replace {
            index: 1,
            old: "thirty".into(),
            new: "sixty".into(),
        }],
    };
    run_with_completer(
        &input,
        "thirty to sixty",
        Some(&output),
        ApplyOptions {
            author: Some("Jane".into()),
            ..ApplyOptions::default()
        },
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap();

    let edited = Docx::open(&output).unwrap();
    let settled = edited
        .edit([blackline_docx::EditOp::RejectAll { author: None }])
        .author("Jane")
        .apply()
        .unwrap()
        .document
        .unwrap();
    assert_eq!(settled.visible_text().unwrap(), original_text);
}

#[tokio::test]
async fn docx_comment_and_insert() {
    let dir = dir();
    let input = dir.path().join("in.docx");
    let output = dir.path().join("out.docx");
    Docx::from_paragraphs(&["Indemnity survives closing."])
        .unwrap()
        .save(&input)
        .unwrap();

    let plan = Plan {
        ops: vec![
            Op::Comment {
                index: 1,
                anchor: "Indemnity".into(),
                text: "Confirm survival period.".into(),
            },
            Op::Insert {
                index: 1,
                position: Position::End,
                text: " The period is two years.".into(),
            },
        ],
    };
    let report = run_with_completer(
        &input,
        "comment indemnity and extend",
        Some(&output),
        ApplyOptions {
            author: Some("Jane".into()),
            ..ApplyOptions::default()
        },
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap();
    assert_eq!(report.apply.failed, 0, "{:?}", report.apply);
    let edited = Docx::open(&output).unwrap();
    assert_eq!(edited.comments().unwrap().len(), 1);
}

#[tokio::test]
async fn insert_after_without_match_swaps_the_phrase() {
    let dir = dir();
    let input = dir.path().join("in.docx");
    let output = dir.path().join("out.docx");
    Docx::from_paragraphs(&["Fees are due within thirty days of invoice date."])
        .unwrap()
        .save(&input)
        .unwrap();

    let plan = Plan {
        ops: vec![Op::Insert {
            index: 1,
            position: Position::After,
            text: "sixty days".into(),
        }],
    };
    let report = run_with_completer(
        &input,
        "change thirty days to sixty days",
        Some(&output),
        ApplyOptions {
            author: Some("Jane".into()),
            ..ApplyOptions::default()
        },
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap();
    assert_eq!(report.apply.failed, 0, "{:?}", report.apply);
    assert_eq!(report.apply.applied, 1);
    let edited = Docx::open(&output).unwrap();
    let text = edited.visible_text().unwrap();
    assert!(text.contains("sixty days"), "{text}");
    assert!(!text.contains("thirty days"), "{text}");
}

#[tokio::test]
async fn xlsx_set_cell() {
    let dir = dir();
    let input = dir.path().join("in.xlsx");
    let output = dir.path().join("out.xlsx");
    Xlsx::from_rows("Cap", &[vec!["Name", "Days"], vec!["Notice", "30"]])
        .unwrap()
        .save(&input)
        .unwrap();

    let plan = Plan {
        ops: vec![Op::SetCell {
            sheet: "Cap".into(),
            cell: "B2".into(),
            value: "60".into(),
        }],
    };
    let report = run_with_completer(
        &input,
        "set days to 60",
        Some(&output),
        ApplyOptions::default(),
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap();
    assert_eq!(report.apply.failed, 0);
    let wb = Xlsx::open(&output).unwrap();
    match wb.cell("Cap", "B2").unwrap() {
        blackline_xlsx::CellValue::Number(n) => assert!((n - 60.0).abs() < f64::EPSILON),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn pptx_set_text() {
    let dir = dir();
    let input = dir.path().join("in.pptx");
    let output = dir.path().join("out.pptx");
    Pptx::create(&CreateSpec {
        slides: vec![SlideSpec {
            texts: vec!["Q2".into(), "Body".into()],
            notes: None,
        }],
    })
    .unwrap()
    .save(&input)
    .unwrap();

    let plan = Plan {
        ops: vec![Op::SetText {
            slide: 1,
            element: 1,
            text: "Q3".into(),
        }],
    };
    let report = run_with_completer(
        &input,
        "title to Q3",
        Some(&output),
        ApplyOptions::default(),
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap();
    assert_eq!(report.apply.failed, 0);
    let deck = Pptx::open(&output).unwrap();
    let view = deck.view().unwrap();
    assert_eq!(
        view.first()
            .and_then(|s| s.elements.first())
            .map(String::as_str),
        Some("Q3")
    );
}

#[tokio::test]
async fn leftover_replace_is_dropped_before_strict_apply() {
    let dir = dir();
    let input = dir.path().join("in.docx");
    let output = dir.path().join("out.docx");
    Docx::from_paragraphs(&["The notice period is thirty days."])
        .unwrap()
        .save(&input)
        .unwrap();

    let plan = Plan {
        ops: vec![
            Op::Replace {
                index: 1,
                old: "thirty".into(),
                new: "sixty".into(),
            },
            Op::Replace {
                index: 1,
                old: "    ".into(),
                new: "    ".into(),
            },
        ],
    };
    let report = run_with_completer(
        &input,
        "thirty to sixty",
        Some(&output),
        ApplyOptions {
            author: Some("Jane".into()),
            ..ApplyOptions::default()
        },
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap();
    assert_eq!(report.apply.failed, 0, "{:?}", report.apply);
    assert_eq!(report.apply.applied, 1);
    assert!(output.exists());
}

#[tokio::test]
async fn leftover_miss_does_not_abort_lenient_apply() {
    let dir = dir();
    let input = dir.path().join("in.docx");
    let output = dir.path().join("out.docx");
    Docx::from_paragraphs(&["The notice period is thirty days."])
        .unwrap()
        .save(&input)
        .unwrap();

    let plan = Plan {
        ops: vec![
            Op::Replace {
                index: 1,
                old: "thirty".into(),
                new: "sixty".into(),
            },
            // Delete is not rewritten by snap, so a miss reaches apply.
            Op::Delete {
                index: 1,
                text: "this text is not in the file".into(),
            },
        ],
    };
    let report = run_with_completer(
        &input,
        "thirty to sixty",
        Some(&output),
        ApplyOptions {
            author: Some("Jane".into()),
            lenient: true,
            ..ApplyOptions::default()
        },
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap();
    assert_eq!(report.apply.applied, 1, "{:?}", report.apply);
    assert_eq!(report.apply.failed, 1);
    assert_eq!(report.apply.mode, "lenient");
    let text = Docx::open(&output).unwrap().visible_text().unwrap();
    assert!(text.contains("sixty days"), "{text}");
}

#[tokio::test]
async fn leftover_miss_aborts_strict_apply() {
    let dir = dir();
    let input = dir.path().join("in.docx");
    let output = dir.path().join("out.docx");
    Docx::from_paragraphs(&["The notice period is thirty days."])
        .unwrap()
        .save(&input)
        .unwrap();

    let plan = Plan {
        ops: vec![
            Op::Replace {
                index: 1,
                old: "thirty".into(),
                new: "sixty".into(),
            },
            Op::Delete {
                index: 1,
                text: "this text is not in the file".into(),
            },
        ],
    };
    let err = run_with_completer(
        &input,
        "thirty to sixty",
        Some(&output),
        ApplyOptions {
            author: Some("Jane".into()),
            ..ApplyOptions::default()
        },
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
    assert!(!output.exists());
}

#[tokio::test]
async fn wrong_op_for_format_fails() {
    let dir = dir();
    let input = dir.path().join("in.xlsx");
    Xlsx::from_rows("S", &[vec!["A"]])
        .unwrap()
        .save(&input)
        .unwrap();
    let plan = Plan {
        ops: vec![Op::Replace {
            index: 1,
            old: "A".into(),
            new: "B".into(),
        }],
    };
    let err = run_with_completer(
        &input,
        "nope",
        None,
        ApplyOptions {
            dry_run: true,
            ..ApplyOptions::default()
        },
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("docx"), "{err}");
}

#[tokio::test]
async fn dry_run_does_not_write() {
    let dir = dir();
    let input = dir.path().join("in.docx");
    let output = dir.path().join("out.docx");
    Docx::from_paragraphs(&["hello"])
        .unwrap()
        .save(&input)
        .unwrap();
    let plan = Plan {
        ops: vec![Op::Replace {
            index: 1,
            old: "hello".into(),
            new: "goodbye".into(),
        }],
    };
    run_with_completer(
        &input,
        "hello to goodbye",
        Some(&output),
        ApplyOptions {
            author: Some("Jane".into()),
            dry_run: true,
            ..ApplyOptions::default()
        },
        &StaticCompleter::new(plan),
    )
    .await
    .unwrap();
    assert!(!output.exists());
}
