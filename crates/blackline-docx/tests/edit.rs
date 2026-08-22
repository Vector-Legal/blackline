//! Create → edit → reopen precision tests for DOCX.

use blackline_docx::{CreateSpec, Docx, EditOp, ParaSpec, SearchQuery, TableSpec};
use blackline_docx::{ParaProps, RunProps};

fn texts(doc: &Docx) -> Vec<String> {
    doc.view(false)
        .unwrap()
        .into_iter()
        .map(|l| l.text)
        .collect()
}

fn apply(doc: Docx, ops: Vec<EditOp>) -> Docx {
    doc.edit(ops)
        .apply()
        .unwrap()
        .document
        .expect("edit produced a document")
}

#[test]
fn create_view_roundtrip() {
    let doc = Docx::from_paragraphs(&["Alpha", "Bravo", "Charlie"]).unwrap();
    assert_eq!(texts(&doc), ["Alpha", "Bravo", "Charlie"]);
    assert!(doc.check().unwrap().passed());
}

#[test]
fn save_and_reopen_preserves_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("roundtrip.docx");
    let doc = Docx::from_paragraphs(&["Keep this sentence.", "And this one."]).unwrap();
    doc.save(&path).unwrap();
    let again = Docx::open(&path).unwrap();
    assert_eq!(again.visible_text().unwrap(), doc.visible_text().unwrap());
    assert!(again.check().unwrap().passed());
}

#[test]
fn plain_replace_by_index() {
    let doc = Docx::from_paragraphs(&["The fee is thirty (30) days."]).unwrap();
    let doc = apply(
        doc,
        vec![EditOp::Replace {
            index: Some(1),
            old: "thirty (30)".into(),
            new: "sixty (60)".into(),
            content_match: None,
        }],
    );
    assert_eq!(texts(&doc), ["The fee is sixty (60) days."]);
    assert!(doc.check().unwrap().passed());
}

#[test]
fn replace_by_content_match() {
    let doc = Docx::from_paragraphs(&["AAA", "Change the Purchase Price here.", "CCC"]).unwrap();
    let doc = apply(
        doc,
        vec![EditOp::Replace {
            index: None,
            old: "Purchase Price".into(),
            new: "Sale Price".into(),
            content_match: Some("Purchase Price".into()),
        }],
    );
    assert_eq!(texts(&doc)[1], "Change the Sale Price here.");
}

#[test]
fn insert_before_and_after() {
    let doc = Docx::from_paragraphs(&["Middle"]).unwrap();
    let doc = apply(
        doc,
        vec![
            EditOp::Insert {
                index: Some(1),
                position: "before".into(),
                content_match: None,
                text: Some("First".into()),
                content: None,
                style: None,
                para_props: ParaProps::default(),
            },
            EditOp::Insert {
                index: Some(2),
                position: "after".into(),
                content_match: None,
                text: Some("Last".into()),
                content: None,
                style: None,
                para_props: ParaProps::default(),
            },
        ],
    );
    assert_eq!(texts(&doc), ["First", "Middle", "Last"]);
}

#[test]
fn insert_paragraph_with_heading_style() {
    let doc = Docx::from_paragraphs(&["Body"]).unwrap();
    let doc = apply(
        doc,
        vec![EditOp::Insert {
            index: Some(1),
            position: "after".into(),
            content_match: None,
            text: Some("Section".into()),
            content: None,
            style: Some("Heading1".into()),
            para_props: ParaProps::default(),
        }],
    );
    let outline = doc.outline().unwrap();
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].text, "Section");
    assert_eq!(outline[0].level, 1);
    assert_eq!(texts(&doc), ["Body", "Section"]);
}

#[test]
fn delete_range() {
    let doc = Docx::from_paragraphs(&["A", "B", "C", "D"]).unwrap();
    let doc = apply(
        doc,
        vec![EditOp::Delete {
            index: None,
            range: Some((2, 3)),
            content_match: None,
        }],
    );
    assert_eq!(texts(&doc), ["A", "D"]);
}

#[test]
fn delete_by_content_match() {
    let doc = Docx::from_paragraphs(&["keep", "drop this", "keep too"]).unwrap();
    let doc = apply(
        doc,
        vec![EditOp::Delete {
            index: None,
            range: None,
            content_match: Some("drop this".into()),
        }],
    );
    assert_eq!(texts(&doc), ["keep", "keep too"]);
}

#[test]
fn delete_run() {
    let doc = Docx::from_paragraphs(&["keep drop keep"]).unwrap();
    // A single-run paragraph: delete_run matches the whole run text.
    let doc = apply(
        doc,
        vec![EditOp::DeleteRun {
            index: Some(1),
            content_match: None,
            text: "keep drop keep".into(),
        }],
    );
    // Paragraph is now empty and drops out of the view.
    assert!(texts(&doc).is_empty());
}

#[test]
fn format_writes_bold() {
    let doc = Docx::from_paragraphs(&["Highlight me"]).unwrap();
    let doc = apply(
        doc,
        vec![EditOp::Format {
            index: 1,
            run_props: RunProps {
                bold: Some(true),
                color: Some("FF0000".into()),
                ..Default::default()
            },
            para_props: ParaProps {
                align: Some("center".into()),
                ..Default::default()
            },
        }],
    );
    let xml = doc
        .package()
        .part_xml("word/document.xml")
        .unwrap()
        .root
        .to_xml_string();
    assert!(xml.contains("<w:b/>") || xml.contains("<w:b "));
    assert!(xml.contains("FF0000"));
    assert!(xml.contains("center"));
}

#[test]
fn format_writes_italic_and_underline() {
    let doc = Docx::from_paragraphs(&["Emphasize"]).unwrap();
    let doc = apply(
        doc,
        vec![EditOp::Format {
            index: 1,
            run_props: RunProps {
                italic: Some(true),
                underline: Some(true),
                ..Default::default()
            },
            para_props: ParaProps::default(),
        }],
    );
    let xml = doc
        .package()
        .part_xml("word/document.xml")
        .unwrap()
        .root
        .to_xml_string();
    assert!(xml.contains("<w:i/>") || xml.contains("<w:i "));
    assert!(xml.contains("<w:u"));
}

#[test]
fn table_insert_and_delete_row() {
    let doc = Docx::create(&CreateSpec {
        font: None,
        paragraphs: vec![ParaSpec {
            text: Some("Caption".into()),
            style: None,
            runs: None,
            props: ParaProps::default(),
        }],
        sections: Vec::new(),
        tables: vec![TableSpec {
            rows: vec![vec!["A".into(), "B".into()], vec!["1".into(), "2".into()]],
        }],
    })
    .unwrap();
    let lines = texts(&doc);
    assert!(lines.iter().any(|t| t.contains("A") && t.contains("B")));

    let doc = apply(
        doc,
        vec![EditOp::TableInsertRow {
            index: 2,
            row_index: 2,
            position: "after".into(),
            cells: vec!["3".into(), "4".into()],
        }],
    );
    let xml = doc
        .package()
        .part_xml("word/document.xml")
        .unwrap()
        .root
        .to_xml_string();
    assert!(xml.contains(">3<"));
    assert_eq!(xml.matches("<w:tr").count(), 3);

    let doc = apply(
        doc,
        vec![EditOp::TableDeleteRow {
            index: 2,
            row_index: 1,
        }],
    );
    let xml = doc
        .package()
        .part_xml("word/document.xml")
        .unwrap()
        .root
        .to_xml_string();
    assert_eq!(xml.matches("<w:tr").count(), 2);
}

#[test]
fn search_is_bounded_and_scoped() {
    let doc = Docx::from_paragraphs(&["alpha", "bravo alpha", "charlie"]).unwrap();
    let hits = doc.search(&SearchQuery::new("alpha").limit(10)).unwrap();
    assert_eq!(hits.total, 2);
    let whole = doc.search(&SearchQuery::new("brav").whole_word()).unwrap();
    assert_eq!(whole.total, 0);
}

#[test]
fn special_characters_survive_roundtrip() {
    let src = [r#"A & B < C > D "quoted""#, "café naïve 日本語 📎"];
    let doc = Docx::from_paragraphs(&src).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("special.docx");
    doc.save(&path).unwrap();
    let again = Docx::open(&path).unwrap();
    assert_eq!(texts(&again), src);
}

#[test]
fn headings_appear_in_outline() {
    let doc = Docx::create(&CreateSpec {
        font: None,
        paragraphs: vec![
            ParaSpec {
                text: Some("Title".into()),
                style: Some("Heading1".into()),
                runs: None,
                props: ParaProps::default(),
            },
            ParaSpec {
                text: Some("Body".into()),
                style: None,
                runs: None,
                props: ParaProps::default(),
            },
        ],
        sections: Vec::new(),
        tables: Vec::new(),
    })
    .unwrap();
    let outline = doc.outline().unwrap();
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].level, 1);
    assert_eq!(outline[0].text, "Title");
    assert_eq!(outline[0].index, 1);
}

#[test]
fn strict_batch_aborts_without_writing() {
    let doc = Docx::from_paragraphs(&["only"]).unwrap();
    let err = doc
        .edit([
            EditOp::Replace {
                index: Some(1),
                old: "only".into(),
                new: "changed".into(),
                content_match: None,
            },
            EditOp::Replace {
                index: Some(9),
                old: "nope".into(),
                new: "x".into(),
                content_match: None,
            },
        ])
        .apply();
    assert!(err.is_err());
}

#[test]
fn lenient_applies_what_it_can() {
    let doc = Docx::from_paragraphs(&["only"]).unwrap();
    let outcome = doc
        .edit([
            EditOp::Replace {
                index: Some(1),
                old: "only".into(),
                new: "changed".into(),
                content_match: None,
            },
            EditOp::Replace {
                index: Some(9),
                old: "nope".into(),
                new: "x".into(),
                content_match: None,
            },
        ])
        .lenient()
        .apply()
        .unwrap();
    assert_eq!(outcome.report.applied, 1);
    assert_eq!(outcome.report.failed, 1);
    assert_eq!(texts(outcome.document.as_ref().unwrap()), ["changed"]);
}

#[test]
fn dry_run_does_not_return_a_document() {
    let doc = Docx::from_paragraphs(&["only"]).unwrap();
    let outcome = doc
        .edit([EditOp::Replace {
            index: Some(1),
            old: "only".into(),
            new: "changed".into(),
            content_match: None,
        }])
        .dry_run()
        .apply()
        .unwrap();
    assert!(outcome.document.is_none());
    assert_eq!(outcome.report.mode, "dry-run");
}

#[test]
fn comment_insert_list_delete_and_pairing() {
    let doc = Docx::from_paragraphs(&["Please review the Closing Date."]).unwrap();
    let doc = doc
        .edit([EditOp::InsertComment {
            index: Some(1),
            content_match: None,
            anchor: Some("Closing Date".into()),
            text: "Check this.".into(),
            author: None,
        }])
        .author("Jamie Chen")
        .apply()
        .unwrap()
        .document
        .unwrap();
    let comments = doc.comments().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].author, "Jamie Chen");
    assert_eq!(comments[0].text, "Check this.");
    assert!(doc.check().unwrap().passed());

    let id = comments[0].id.clone();
    let doc = apply(doc, vec![EditOp::DeleteComment { id }]);
    assert!(doc.comments().unwrap().is_empty());
    assert!(doc.check().unwrap().passed());
}

#[test]
fn comment_requires_author() {
    let doc = Docx::from_paragraphs(&["Hello"]).unwrap();
    let err = doc
        .edit([EditOp::InsertComment {
            index: Some(1),
            content_match: None,
            anchor: None,
            text: "note".into(),
            author: None,
        }])
        .apply();
    assert!(err.is_err());
}

#[test]
fn info_counts_match_view() {
    let doc = Docx::from_paragraphs(&["one", "two"]).unwrap();
    let info = doc.info().unwrap();
    assert_eq!(info.paragraphs, 2);
    assert_eq!(info.tables, 0);
    assert_eq!(info.tracked_changes.total, 0);
}
