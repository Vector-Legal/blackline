//! Track module: multi-author redline, surgical delete, minimize, comments.

use std::path::PathBuf;

use blackline_docx::{track_redline, Docx, Granularity, TrackOp, TrackedChange};

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus/docx")
        .join(name)
}

fn visible(doc: &Docx) -> String {
    doc.visible_text().unwrap()
}

fn xml(doc: &Docx) -> String {
    doc.package()
        .part_xml("word/document.xml")
        .unwrap()
        .root
        .to_xml_string()
}

fn apply(doc: Docx, ops: Vec<TrackOp>) -> Docx {
    let outcome = doc.track(ops).apply().unwrap();
    assert_eq!(outcome.report.failed, 0, "{:?}", outcome.report);
    outcome.document.expect("track produced a document")
}

fn apply_default(doc: Docx, ops: Vec<TrackOp>, author: &str) -> Docx {
    let outcome = doc.track(ops).author(author).apply().unwrap();
    assert_eq!(outcome.report.failed, 0, "{:?}", outcome.report);
    outcome.document.expect("track produced a document")
}

fn settle(doc: Docx, accept: bool, author: Option<&str>) -> Docx {
    let op = if accept {
        TrackOp::AcceptAll {
            author: author.map(str::to_string),
        }
    } else {
        TrackOp::RejectAll {
            author: author.map(str::to_string),
        }
    };
    apply(doc, vec![op])
}

fn replace(old: &str, new: &str, author: &str) -> TrackOp {
    TrackOp::Replace {
        index: None,
        content_match: Some(old.into()),
        old: Some(old.into()),
        new: new.into(),
        author: Some(author.into()),
        date: None,
    }
}

fn deleted_text(changes: &[TrackedChange]) -> String {
    changes
        .iter()
        .filter(|c| c.kind == "delete")
        .map(|c| c.text.as_str())
        .collect()
}

fn inserted_text(changes: &[TrackedChange]) -> String {
    changes
        .iter()
        .filter(|c| c.kind == "insert")
        .map(|c| c.text.as_str())
        .collect()
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn minimize_marks_only_the_changed_word() {
    let doc = Docx::from_paragraphs(&["the cat sat"]).unwrap();
    let edited = apply(doc, vec![replace("the cat sat", "the dog sat", "Jane")]);
    assert_eq!(visible(&edited), "the dog sat");
    let changes = edited.changes().unwrap();
    assert_eq!(deleted_text(&changes).trim(), "cat");
    assert_eq!(inserted_text(&changes).trim(), "dog");
    let markup = xml(&edited);
    assert!(markup.contains("<w:del"));
    assert!(markup.contains("<w:ins"));
    assert!(
        !markup.contains(">the cat<") && !deleted_text(&changes).contains("the"),
        "minimize leaked unchanged 'the' into a delete: {changes:?}"
    );
    assert!(edited.check().unwrap().passed());
}

#[test]
fn identical_delete_insert_cancels() {
    let doc = Docx::from_paragraphs(&["don't touch this"]).unwrap();
    let edited = apply(
        doc,
        vec![replace("don't touch this", "don't touch this", "Jane")],
    );
    assert_eq!(visible(&edited), "don't touch this");
    assert!(
        edited.changes().unwrap().is_empty(),
        "no-op replace must not emit revisions: {:?}",
        edited.changes().unwrap()
    );
}

#[test]
fn smart_quote_no_op_cancels() {
    let doc = Docx::from_paragraphs(&["don't"]).unwrap();
    let edited = apply(doc, vec![replace("don't", "don\u{2019}t", "Jane")]);
    assert!(
        edited.changes().unwrap().is_empty(),
        "normalized-equal replace must cancel: {:?}",
        edited.changes().unwrap()
    );
}

#[test]
fn surgical_delete_keeps_surrounding_text() {
    let doc = Docx::from_paragraphs(&["keep ALPHA keep"]).unwrap();
    let edited = apply(
        doc,
        vec![TrackOp::Delete {
            index: Some(1),
            content_match: None,
            text: Some("ALPHA".into()),
            old: None,
            author: Some("Jane".into()),
            date: None,
        }],
    );
    assert_eq!(visible(&edited), "keep  keep");
    let changes = edited.changes().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, "delete");
    assert_eq!(changes[0].text, "ALPHA");
    assert!(!xml(&edited).contains("<w:ins"));
    let rejected = settle(edited, false, None);
    assert_eq!(visible(&rejected), "keep ALPHA keep");
}

#[test]
fn surgical_delete_keeps_hyperlink() {
    let original = Docx::open(corpus("TestDocument.docx")).unwrap();
    let before = original.hyperlinks().unwrap();
    assert!(
        before.iter().any(|l| l.target.contains("poi.apache.org")),
        "{before:?}"
    );
    let edited = apply(
        original,
        vec![TrackOp::Delete {
            index: None,
            content_match: Some("test".into()),
            text: Some("test".into()),
            old: None,
            author: Some("Jane".into()),
            date: None,
        }],
    );
    let links = edited.hyperlinks().unwrap();
    assert_eq!(
        links.len(),
        before.len(),
        "surgical delete dropped a hyperlink"
    );
    assert!(
        xml(&edited).contains("<w:hyperlink"),
        "hyperlink wrapper vanished"
    );
    assert!(edited.check().unwrap().passed());
}

#[test]
fn multi_author_replace_and_filter() {
    let doc = Docx::from_paragraphs(&["The notice period is thirty (30) days."]).unwrap();
    let edited = apply(
        doc,
        vec![
            replace("thirty (30)", "sixty (60)", "Jane"),
            TrackOp::Insert {
                index: None,
                content_match: Some("days".into()),
                position: "before".into(),
                text: "calendar ".into(),
                author: Some("Bob".into()),
                date: None,
            },
        ],
    );
    assert_eq!(
        visible(&edited),
        "The notice period is sixty (60) calendar days."
    );
    let changes = edited.changes().unwrap();
    assert!(changes.iter().any(|c| c.author == "Jane"));
    assert!(changes.iter().any(|c| c.author == "Bob"));
    let jane: Vec<_> = changes.iter().filter(|c| c.author == "Jane").collect();
    let bob: Vec<_> = changes.iter().filter(|c| c.author == "Bob").collect();
    assert!(jane.iter().any(|c| c.kind == "delete"));
    assert!(jane.iter().any(|c| c.kind == "insert"));
    assert!(bob.iter().all(|c| c.kind == "insert"));
}

#[test]
fn comment_and_track_on_same_paragraph() {
    let doc = Docx::from_paragraphs(&["The notice period is thirty (30) days."]).unwrap();
    let edited = apply(
        doc,
        vec![
            replace("thirty (30)", "sixty (60)", "Jane"),
            TrackOp::Comment {
                index: None,
                content_match: Some("sixty (60)".into()),
                anchor: Some("sixty (60)".into()),
                text: "Confirm with counsel.".into(),
                author: Some("Jane".into()),
                date: Some("2026-01-15T12:00:00Z".into()),
            },
        ],
    );
    assert_eq!(visible(&edited), "The notice period is sixty (60) days.");
    let comments = edited.comments().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].author, "Jane");
    assert_eq!(comments[0].text, "Confirm with counsel.");
    assert_eq!(comments[0].date, "2026-01-15T12:00:00Z");
    assert!(edited.check().unwrap().passed());
}

#[test]
fn accept_one_author_reject_another() {
    let doc = Docx::from_paragraphs(&["alpha beta gamma"]).unwrap();
    let edited = apply(
        doc,
        vec![
            replace("alpha", "ALPHA", "Jane"),
            replace("gamma", "GAMMA", "Bob"),
        ],
    );
    assert_eq!(visible(&edited), "ALPHA beta GAMMA");
    let after_jane = settle(edited, true, Some("Jane"));
    assert_eq!(visible(&after_jane), "ALPHA beta GAMMA");
    let jane_gone = after_jane
        .changes()
        .unwrap()
        .into_iter()
        .all(|c| c.author != "Jane");
    assert!(jane_gone);
    let rejected_bob = settle(after_jane, false, Some("Bob"));
    assert_eq!(visible(&rejected_bob), "ALPHA beta gamma");
    assert!(rejected_bob.changes().unwrap().is_empty());
}

#[test]
fn reject_all_is_original_accept_all_is_revised() {
    let original_text = "The notice period is thirty (30) days.";
    let doc = Docx::from_paragraphs(&[original_text]).unwrap();
    let edited = apply(doc, vec![replace("thirty (30)", "sixty (60)", "Jane")]);
    assert_eq!(visible(&edited), "The notice period is sixty (60) days.");

    let rejected = settle(edited.clone_via_bytes(), false, None);
    assert_eq!(visible(&rejected), original_text);
    assert!(rejected.changes().unwrap().is_empty());

    let accepted = settle(edited, true, None);
    assert_eq!(visible(&accepted), "The notice period is sixty (60) days.");
    assert!(accepted.changes().unwrap().is_empty());
}

#[test]
fn custom_date_stamps_ins_and_del() {
    let doc = Docx::from_paragraphs(&["hello world"]).unwrap();
    let edited = apply(
        doc,
        vec![TrackOp::Replace {
            index: Some(1),
            content_match: None,
            old: Some("hello".into()),
            new: "howdy".into(),
            author: Some("Jane".into()),
            date: Some("2024-06-01T09:30:00Z".into()),
        }],
    );
    for change in edited.changes().unwrap() {
        assert_eq!(change.date, "2024-06-01T09:30:00Z");
        assert_eq!(change.author, "Jane");
    }
}

#[test]
fn insert_paragraph_is_tracked() {
    let doc = Docx::from_paragraphs(&["first", "third"]).unwrap();
    let edited = apply(
        doc,
        vec![TrackOp::InsertParagraph {
            index: Some(1),
            content_match: None,
            position: "after".into(),
            text: "second".into(),
            author: Some("Bob".into()),
            date: None,
        }],
    );
    assert_eq!(visible(&edited), "first\nsecond\nthird");
    assert!(edited
        .changes()
        .unwrap()
        .iter()
        .any(|c| c.kind == "insert" && c.text.contains("second") && c.author == "Bob"));
    let rejected = settle(edited, false, None);
    assert_eq!(visible(&rejected), "first\nthird");
}

#[test]
fn delete_whole_paragraph_marks_visible_text() {
    let doc = Docx::from_paragraphs(&["keep", "drop me", "keep"]).unwrap();
    let edited = apply(
        doc,
        vec![TrackOp::Delete {
            index: Some(2),
            content_match: None,
            text: None,
            old: None,
            author: Some("Jane".into()),
            date: None,
        }],
    );
    // Visible text skips w:del, so the paragraph is empty but still present.
    let lines = edited.text_lines().unwrap();
    assert!(lines.iter().any(|l| l.contains("keep")));
    assert!(!visible(&edited).contains("drop me"));
    assert!(edited
        .changes()
        .unwrap()
        .iter()
        .any(|c| c.kind == "delete" && c.text.contains("drop me")));
    let rejected = settle(edited, false, None);
    assert!(visible(&rejected).contains("drop me"));
}

#[test]
fn track_redline_precision_and_minimize() {
    let original =
        Docx::from_paragraphs(&["the cat sat on the mat", "Unchanged paragraph."]).unwrap();
    let revised = Docx::from_paragraphs(&[
        "the dog sat on the mat",
        "Unchanged paragraph.",
        "Brand new paragraph.",
    ])
    .unwrap();
    let red = track_redline(&original, &revised, "Morgan Lee", Granularity::Word).unwrap();
    assert!(red.check_against(&original).unwrap().passed());
    let deleted = deleted_text(&red.changes().unwrap());
    assert!(
        deleted.contains("cat"),
        "expected cat to be deleted: {deleted:?}"
    );
    assert!(
        !deleted.contains("sat"),
        "minimize should not delete 'sat': {deleted:?}"
    );

    let rejected = settle(red.clone_via_bytes(), false, None);
    assert_eq!(
        normalize(&visible(&rejected)),
        normalize(&visible(&original))
    );

    let accepted = settle(red, true, None);
    assert_eq!(
        normalize(&visible(&accepted)),
        normalize(&visible(&revised))
    );
}

#[test]
fn author_required() {
    let doc = Docx::from_paragraphs(&["hello"]).unwrap();
    let err = doc
        .track([TrackOp::Replace {
            index: Some(1),
            content_match: None,
            old: Some("hello".into()),
            new: "hi".into(),
            author: None,
            date: None,
        }])
        .apply();
    assert!(err.is_err(), "missing author must fail");
}

#[test]
fn dry_run_writes_nothing() {
    let doc = Docx::from_paragraphs(&["hello world"]).unwrap();
    let outcome = doc
        .track([replace("hello", "howdy", "Jane")])
        .dry_run()
        .apply()
        .unwrap();
    assert_eq!(outcome.report.applied, 1);
    assert!(outcome.document.is_none());
}

#[test]
fn default_author_fills_ops_without_one() {
    let doc = Docx::from_paragraphs(&["hello world"]).unwrap();
    let edited = apply_default(
        doc,
        vec![TrackOp::Replace {
            index: Some(1),
            content_match: None,
            old: Some("hello".into()),
            new: "howdy".into(),
            author: None,
            date: None,
        }],
        "Default Author",
    );
    assert!(edited
        .changes()
        .unwrap()
        .iter()
        .all(|c| c.author == "Default Author"));
}

#[test]
fn accept_and_reject_single_ids() {
    let doc = Docx::from_paragraphs(&["aa bb cc"]).unwrap();
    let edited = apply(doc, vec![replace("bb", "XX", "Jane")]);
    let ids: Vec<String> = edited
        .changes()
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert!(!ids.is_empty());
    let after = apply(
        edited,
        ids.into_iter().map(|id| TrackOp::Accept { id }).collect(),
    );
    assert_eq!(visible(&after), "aa XX cc");
    assert!(after.changes().unwrap().is_empty());
}

#[test]
fn package_health_after_track_on_poi_sample() {
    let original = Docx::open(corpus("sample.docx")).unwrap();
    let edited = apply(original, vec![replace("Lorem", "LoremX", "Casey Ng")]);
    assert!(visible(&edited).contains("LoremX"));
    assert!(edited.check().unwrap().passed());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tracked.docx");
    edited.save(&path).unwrap();
    let again = Docx::open(&path).unwrap();
    assert!(again.check().unwrap().passed());
    assert_eq!(visible(&again), visible(&edited));
}

trait CloneViaBytes {
    fn clone_via_bytes(&self) -> Self;
}

impl CloneViaBytes for Docx {
    fn clone_via_bytes(&self) -> Self {
        Docx::from_bytes(self.to_bytes().unwrap()).unwrap()
    }
}
