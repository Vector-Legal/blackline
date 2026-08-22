//! End-to-end edit and redline precision against vendored Apache POI DOCX files.
//!
//! Precision rule: after a tracked edit or a two-document redline,
//! reject-all must reproduce the original visible text and accept-all
//! must reproduce the revised visible text. `check` must pass after
//! every successful mutation.

use std::path::PathBuf;

use blackline_docx::{Docx, EditOp, Granularity, ParaProps, SearchQuery};

const FILES: &[&str] = &[
    "sample.docx",
    "SampleDoc.docx",
    "TestDocument.docx",
    "heading123.docx",
    "Styles.docx",
    "TestTableCellAlign.docx",
    "Numbering.docx",
    "ComplexNumberedLists.docx",
    "HeaderFooterUnicode.docx",
    "testComment.docx",
    "delins.docx",
    "FieldCodes.docx",
    "footnotes.docx",
    "VariousPictures.docx",
    "bookmarks.docx",
];

const AUTHOR: &str = "Corpus Tester";
const INSERT_MARKER: &str = "BLACKLINE_CORPUS_INSERT";

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/docx")
}

fn corpus_path(name: &str) -> PathBuf {
    corpus_dir().join(name)
}

fn open(name: &str) -> Docx {
    let path = corpus_path(name);
    assert!(
        path.is_file(),
        "missing corpus file {} (workspace checkout required)",
        path.display()
    );
    Docx::open(&path).unwrap_or_else(|e| panic!("open {name}: {e}"))
}

fn visible(doc: &Docx) -> String {
    doc.visible_text().unwrap()
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn apply(doc: Docx, ops: Vec<EditOp>) -> Docx {
    doc.edit(ops)
        .apply()
        .unwrap_or_else(|e| panic!("edit failed: {e}"))
        .document
        .expect("edit produced a document")
}

fn apply_tracked(doc: Docx, ops: Vec<EditOp>, gran: Granularity) -> Docx {
    doc.edit(ops)
        .tracked(AUTHOR)
        .granularity(gran)
        .apply()
        .unwrap_or_else(|e| panic!("tracked edit failed: {e}"))
        .document
        .expect("tracked edit produced a document")
}

fn settle(doc: Docx, accept: bool) -> Docx {
    let op = if accept {
        EditOp::AcceptAll { author: None }
    } else {
        EditOp::RejectAll { author: None }
    };
    apply(doc, vec![op])
}

fn clone_doc(doc: &Docx) -> Docx {
    Docx::from_bytes(doc.to_bytes().unwrap()).unwrap()
}

/// Lock current visible text by accepting existing Word revisions.
/// Required before a two-document redline on `delins.docx`.
fn flatten(doc: Docx) -> Docx {
    if doc.changes().unwrap().is_empty() {
        return doc;
    }
    let flat = settle(doc, true);
    assert!(
        flat.changes().unwrap().is_empty(),
        "flatten left tracked changes"
    );
    assert!(flat.check().unwrap().passed(), "flatten failed check");
    flat
}

fn insert_after(index: usize, text: &str) -> EditOp {
    EditOp::Insert {
        index: Some(index),
        position: "after".into(),
        content_match: None,
        text: Some(text.into()),
        content: None,
        style: None,
        para_props: ParaProps::default(),
    }
}

fn replace_at(index: usize, old: &str, new: &str) -> EditOp {
    EditOp::Replace {
        index: Some(index),
        old: old.into(),
        new: new.into(),
        content_match: None,
    }
}

fn pick_token(text: &str) -> Option<String> {
    text.split(|c: char| !c.is_alphabetic())
        .find(|w| w.chars().count() >= 4)
        .map(str::to_string)
}

/// First paragraph (not table) with a 4+ letter alphabetic token.
fn first_replace_target(doc: &Docx) -> Option<(usize, String)> {
    for line in doc.view(false).unwrap() {
        if line.kind != "paragraph" {
            continue;
        }
        if let Some(token) = pick_token(&line.text) {
            return Some((line.index, token));
        }
    }
    None
}

fn last_view_index(doc: &Docx) -> usize {
    doc.view(false)
        .unwrap()
        .into_iter()
        .map(|l| l.index)
        .max()
        .expect("document has no view lines")
}

fn assert_check(doc: &Docx, label: &str) {
    let health = doc.check().unwrap();
    assert!(
        health.passed(),
        "{label}: check failed: {}",
        serde_json::to_string_pretty(&health).unwrap_or_else(|_| format!("{health:?}"))
    );
}

fn assert_redline_precision(original: &Docx, revised: &Docx, label: &str) {
    let red = blackline_docx::redline(original, revised, AUTHOR, Granularity::Word)
        .unwrap_or_else(|e| panic!("{label}: redline: {e}"));
    assert_check(&red, &format!("{label} redline"));
    let against = red
        .check_against(original)
        .unwrap_or_else(|e| panic!("{label}: check_against: {e}"));
    assert!(
        against.passed(),
        "{label}: check_against failed: {}",
        serde_json::to_string_pretty(&against).unwrap_or_else(|_| format!("{against:?}"))
    );

    let rejected = settle(clone_doc(&red), false);
    assert_eq!(
        normalize(&visible(&rejected)),
        normalize(&visible(original)),
        "{label}: reject-all != original visible text"
    );
    assert!(
        rejected.changes().unwrap().is_empty(),
        "{label}: reject-all left changes"
    );
    assert_check(&rejected, &format!("{label} rejected"));

    let accepted = settle(red, true);
    assert_eq!(
        normalize(&visible(&accepted)),
        normalize(&visible(revised)),
        "{label}: accept-all != revised visible text"
    );
    assert!(
        accepted.changes().unwrap().is_empty(),
        "{label}: accept-all left changes"
    );
    assert_check(&accepted, &format!("{label} accepted"));
}

fn assert_tracked_precision(original: &Docx, edited: Docx, label: &str) {
    assert_check(&edited, &format!("{label} tracked"));
    let changes = edited.changes().unwrap();
    assert!(
        !changes.is_empty(),
        "{label}: tracked edit produced no changes"
    );
    assert!(
        changes.iter().all(|c| c.author == AUTHOR),
        "{label}: unexpected change author"
    );

    let rejected = settle(clone_doc(&edited), false);
    assert_eq!(
        normalize(&visible(&rejected)),
        normalize(&visible(original)),
        "{label}: tracked reject-all != original"
    );
    assert_check(&rejected, &format!("{label} tracked rejected"));

    let accepted = settle(edited, true);
    assert_ne!(
        normalize(&visible(&accepted)),
        normalize(&visible(original)),
        "{label}: tracked accept-all equals original (edit was a no-op)"
    );
    assert_check(&accepted, &format!("{label} tracked accepted"));
}

fn assert_sidecar_parts_untouched(before: &Docx, after: &Docx, label: &str) {
    for (name, bytes) in before.package().iter() {
        let keep = name.contains("/media/")
            || name.contains("header")
            || name.contains("footer")
            || name.contains("footnotes")
            || name.contains("endnotes")
            || name.ends_with(".jpeg")
            || name.ends_with(".jpg")
            || name.ends_with(".png")
            || name.ends_with(".wmf")
            || name.ends_with(".emf")
            || name.ends_with(".pict");
        if !keep {
            continue;
        }
        let after_bytes = after
            .package()
            .part(name)
            .unwrap_or_else(|_| panic!("{label}: missing sidecar part {name}"));
        assert_eq!(
            after_bytes, bytes,
            "{label}: sidecar part {name} was rewritten"
        );
    }
}

fn edit_by_replace_or_insert(doc: &Docx) -> (Docx, &'static str) {
    if doc.comments().unwrap().is_empty() {
        if let Some((index, token)) = first_replace_target(doc) {
            let replacement = format!("{token}X");
            let revised = apply(
                clone_doc(doc),
                vec![replace_at(index, &token, &replacement)],
            );
            return (revised, "replace");
        }
    }
    let index = last_view_index(doc);
    let revised = apply(clone_doc(doc), vec![insert_after(index, INSERT_MARKER)]);
    (revised, "insert")
}

#[test]
fn corpus_files_are_present() {
    let dir = corpus_dir();
    assert!(dir.is_dir(), "corpus dir missing: {}", dir.display());
    for name in FILES {
        assert!(corpus_path(name).is_file(), "missing {name}");
    }
    let extra: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".docx") && !FILES.contains(&n.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "unexpected extra corpus files (update FILES): {extra:?}"
    );
}

#[test]
fn every_corpus_file_opens_views_and_checks() {
    for name in FILES {
        let doc = open(name);
        let lines = doc.view(false).unwrap();
        assert!(
            !lines.is_empty(),
            "{name}: view is empty (file is not useful for body edits)"
        );
        assert_check(&doc, name);
        let info = doc.info().unwrap();
        assert!(
            info.paragraphs + info.tables > 0,
            "{name}: info reports no body elements"
        );
        let _ = doc.outline().unwrap();
        let _ = doc.search(&SearchQuery::new("the")).unwrap();
    }
}

#[test]
fn every_file_plain_edit_then_two_document_redline() {
    for name in FILES {
        let original = flatten(open(name));
        let before_sidecars = clone_doc(&original);
        let (revised, kind) = edit_by_replace_or_insert(&original);
        assert_check(&revised, &format!("{name} {kind}"));
        assert_ne!(
            normalize(&visible(&revised)),
            normalize(&visible(&original)),
            "{name}: {kind} did not change visible text"
        );
        if kind == "insert" {
            assert!(
                visible(&revised).contains(INSERT_MARKER),
                "{name}: inserted marker missing"
            );
        }
        assert_sidecar_parts_untouched(&before_sidecars, &revised, name);
        assert_redline_precision(&original, &revised, &format!("{name} {kind}"));
    }
}

#[test]
fn every_file_tracked_edit_accept_reject() {
    for name in FILES {
        let original = flatten(open(name));
        let ops = if original.comments().unwrap().is_empty() {
            if let Some((index, token)) = first_replace_target(&original) {
                vec![replace_at(index, &token, &format!("{token}X"))]
            } else {
                vec![insert_after(last_view_index(&original), INSERT_MARKER)]
            }
        } else {
            vec![insert_after(last_view_index(&original), INSERT_MARKER)]
        };
        let edited = apply_tracked(clone_doc(&original), ops, Granularity::Word);
        assert_tracked_precision(&original, edited, name);
    }
}

#[test]
fn sample_redline_at_char_word_and_sentence() {
    let original = open("sample.docx");
    let (index, token) = first_replace_target(&original).expect("sample.docx has a token");
    let replacement = format!("{token}X");
    let revised = apply(
        clone_doc(&original),
        vec![replace_at(index, &token, &replacement)],
    );
    for gran in [Granularity::Char, Granularity::Word, Granularity::Sentence] {
        let red = blackline_docx::redline(&original, &revised, AUTHOR, gran).unwrap();
        assert_check(&red, &format!("sample {gran:?}"));
        assert!(red.check_against(&original).unwrap().passed());
        let rejected = settle(clone_doc(&red), false);
        assert_eq!(
            normalize(&visible(&rejected)),
            normalize(&visible(&original)),
            "sample {gran:?}: reject-all"
        );
        let accepted = settle(red, true);
        assert_eq!(
            normalize(&visible(&accepted)),
            normalize(&visible(&revised)),
            "sample {gran:?}: accept-all"
        );
    }
}

#[test]
fn delins_lists_existing_word_revisions() {
    let doc = open("delins.docx");
    let changes = doc.changes().unwrap();
    assert_eq!(changes.len(), 36, "POI delins.docx revision count drifted");
    assert!(changes.iter().any(|c| c.kind == "insert"));
    assert!(changes.iter().any(|c| c.kind == "delete"));
    assert!(changes.iter().all(|c| c.author == "pavel"));
    assert_check(&doc, "delins original");

    let accepted = settle(clone_doc(&doc), true);
    let rejected = settle(doc, false);
    assert_ne!(
        normalize(&visible(&accepted)),
        normalize(&visible(&rejected)),
        "accept_all and reject_all of existing Word markup should differ"
    );
    assert!(accepted.changes().unwrap().is_empty());
    assert!(rejected.changes().unwrap().is_empty());
    assert_check(&accepted, "delins accepted existing");
    assert_check(&rejected, "delins rejected existing");
}

#[test]
fn test_comment_lists_and_roundtrips() {
    let doc = open("testComment.docx");
    let comments = doc.comments().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].author, "poi");
    assert_eq!(comments[0].text, "comment content");
    assert_check(&doc, "testComment original");

    let with_note = doc
        .edit([EditOp::InsertComment {
            index: Some(1),
            content_match: None,
            anchor: Some("paragraph".into()),
            text: "second note".into(),
            author: None,
        }])
        .author(AUTHOR)
        .apply()
        .unwrap()
        .document
        .unwrap();
    let listed = with_note.comments().unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().any(|c| c.text == "second note"));
    assert_check(&with_note, "testComment after insert");

    let extra = listed
        .iter()
        .find(|c| c.text == "second note")
        .unwrap()
        .id
        .clone();
    let after_delete = apply(with_note, vec![EditOp::DeleteComment { id: extra }]);
    assert_eq!(after_delete.comments().unwrap().len(), 1);
    assert_eq!(after_delete.comments().unwrap()[0].author, "poi");
    assert_check(&after_delete, "testComment after delete");
}

#[test]
fn heading123_outline_and_styles_view() {
    let headings = open("heading123.docx").outline().unwrap();
    assert_eq!(headings.len(), 3);
    assert_eq!(headings[0].level, 1);
    assert_eq!(headings[1].level, 2);
    assert_eq!(headings[2].level, 3);
    assert!(headings[0].text.contains("First paragraph"));

    let styles = open("Styles.docx");
    let texts: Vec<_> = styles
        .view(false)
        .unwrap()
        .into_iter()
        .map(|l| l.text)
        .collect();
    assert!(texts.iter().any(|t| t.contains("Heading1")));
    assert!(texts.iter().any(|t| t.contains("Standard")));
}

#[test]
fn table_row_insert_and_delete() {
    let doc = open("TestTableCellAlign.docx");
    let before = visible(&doc);
    assert!(before.contains("Top"));
    assert!(before.contains("Center"));
    assert_eq!(doc.info().unwrap().tables, 1);

    let inserted = apply(
        clone_doc(&doc),
        vec![EditOp::TableInsertRow {
            index: 1,
            row_index: 1,
            position: "after".into(),
            cells: vec!["LEFT".into(), "RIGHT".into()],
        }],
    );
    assert!(visible(&inserted).contains("LEFT"));
    assert!(visible(&inserted).contains("RIGHT"));
    assert_check(&inserted, "table insert row");

    let deleted = apply(
        inserted,
        vec![EditOp::TableDeleteRow {
            index: 1,
            row_index: 2,
        }],
    );
    assert!(!visible(&deleted).contains("LEFT"));
    assert_eq!(normalize(&visible(&deleted)), normalize(&before));
    assert_check(&deleted, "table delete row");
}

#[test]
fn pictures_and_headers_survive_body_edit() {
    for name in [
        "VariousPictures.docx",
        "HeaderFooterUnicode.docx",
        "footnotes.docx",
    ] {
        let original = open(name);
        let (revised, _) = edit_by_replace_or_insert(&original);
        assert_sidecar_parts_untouched(&original, &revised, name);
        assert_check(&revised, name);
    }
}

#[test]
fn delete_paragraph_redline_restores_it() {
    let original = open("SampleDoc.docx");
    let last = last_view_index(&original);
    let revised = apply(
        clone_doc(&original),
        vec![EditOp::Delete {
            index: Some(last),
            range: None,
            content_match: None,
        }],
    );
    assert!(visible(&revised).len() < visible(&original).len());
    assert_check(&revised, "SampleDoc delete");
    assert_redline_precision(&original, &revised, "SampleDoc delete");
}

#[test]
fn save_reopen_after_redline() {
    let original = flatten(open("TestDocument.docx"));
    let (revised, _) = edit_by_replace_or_insert(&original);
    let red = blackline_docx::redline(&original, &revised, AUTHOR, Granularity::Word).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("roundtrip.docx");
    red.save(&path).unwrap();
    let again = Docx::open(&path).unwrap();
    assert_eq!(normalize(&visible(&again)), normalize(&visible(&red)));
    assert_check(&again, "reopened redline");
    assert!(again.check_against(&original).unwrap().passed());
}

#[test]
fn find_hits_real_body_text() {
    let doc = open("sample.docx");
    let hits = doc.search(&SearchQuery::new("Lorem")).unwrap();
    assert!(hits.total >= 1, "sample.docx should contain Lorem");
    assert!(hits.matches.iter().any(|h| h.text.contains("Lorem")));
}
