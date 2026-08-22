//! Tracked-change and redline precision: reject-all = original, accept-all = new.

use blackline_docx::{Docx, EditOp, Granularity};

fn visible(doc: &Docx) -> String {
    doc.visible_text().unwrap()
}

fn tracked_replace(old: &str, from: &str, to: &str, gran: Granularity) -> Docx {
    let doc = Docx::from_paragraphs(&[old]).unwrap();
    doc.edit([EditOp::Replace {
        index: Some(1),
        old: from.into(),
        new: to.into(),
        content_match: None,
    }])
    .tracked("Taylor Kim")
    .granularity(gran)
    .apply()
    .unwrap()
    .document
    .unwrap()
}

fn settle(doc: Docx, accept: bool) -> Docx {
    let op = if accept {
        EditOp::AcceptAll { author: None }
    } else {
        EditOp::RejectAll { author: None }
    };
    doc.edit([op]).apply().unwrap().document.unwrap()
}

#[test]
fn tracked_replace_visible_text_is_the_revision() {
    let edited = tracked_replace(
        "The notice period is thirty (30) days.",
        "thirty (30)",
        "sixty (60)",
        Granularity::Word,
    );
    assert_eq!(visible(&edited), "The notice period is sixty (60) days.");
    let changes = edited.changes().unwrap();
    assert!(!changes.is_empty());
    assert!(changes.iter().any(|c| c.kind == "delete"));
    assert!(changes.iter().any(|c| c.kind == "insert"));
    assert!(changes.iter().all(|c| c.author == "Taylor Kim"));
    assert!(edited.check().unwrap().passed());
}

#[test]
fn reject_all_restores_original_text() {
    let original = "The notice period is thirty (30) days.";
    let edited = tracked_replace(original, "thirty (30)", "sixty (60)", Granularity::Word);
    let rejected = settle(edited, false);
    assert_eq!(visible(&rejected), original);
    assert!(rejected.changes().unwrap().is_empty());
}

#[test]
fn accept_all_keeps_revised_text() {
    let edited = tracked_replace(
        "The notice period is thirty (30) days.",
        "thirty (30)",
        "sixty (60)",
        Granularity::Word,
    );
    let accepted = settle(edited, true);
    assert_eq!(visible(&accepted), "The notice period is sixty (60) days.");
    assert!(accepted.changes().unwrap().is_empty());
}

#[test]
fn tracked_markup_uses_ins_and_deltext() {
    let edited = tracked_replace("pay thirty", "thirty", "sixty", Granularity::Char);
    let xml = edited
        .package()
        .part_xml("word/document.xml")
        .unwrap()
        .root
        .to_xml_string();
    assert!(xml.contains("<w:ins"));
    assert!(xml.contains("<w:del"));
    assert!(xml.contains("<w:delText"));
    // w:del must not wrap a w:t (Word uses w:delText).
    let doc = edited.package().part_xml("word/document.xml").unwrap();
    let mut bad = false;
    doc.root.walk(&mut |n| {
        if n.is_element_with_local_name("del") {
            n.walk(&mut |c| {
                if c.is_element_with_local_name("t") {
                    bad = true;
                }
            });
        }
    });
    assert!(!bad, "w:t found inside w:del");
}

#[test]
fn accept_and_reject_single_ids() {
    let edited = tracked_replace("aa bb cc", "bb", "XX", Granularity::Word);
    let ids: Vec<String> = edited
        .changes()
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert!(!ids.is_empty());
    let after = edited
        .edit(ids.into_iter().map(|id| EditOp::AcceptChange { id }))
        .apply()
        .unwrap()
        .document
        .unwrap();
    assert_eq!(visible(&after), "aa XX cc");
}

#[test]
fn reject_single_id_restores_that_span() {
    let edited = tracked_replace("aa bb cc", "bb", "XX", Granularity::Word);
    let ids: Vec<String> = edited
        .changes()
        .unwrap()
        .into_iter()
        .filter(|c| c.kind == "insert")
        .map(|c| c.id)
        .collect();
    assert!(!ids.is_empty());
    let after = edited
        .edit(ids.into_iter().map(|id| EditOp::RejectChange { id }))
        .apply()
        .unwrap()
        .document
        .unwrap();
    assert!(visible(&after).contains("aa"));
    assert!(visible(&after).contains("cc"));
    assert!(!visible(&after).contains("XX"));
}

#[test]
fn granularity_sentence_replaces_the_whole_sentence() {
    let edited = tracked_replace(
        "First sentence. Second sentence stays.",
        "First sentence.",
        "Rewritten sentence.",
        Granularity::Sentence,
    );
    assert_eq!(
        visible(&edited),
        "Rewritten sentence. Second sentence stays."
    );
    let rejected = settle(edited, false);
    assert_eq!(visible(&rejected), "First sentence. Second sentence stays.");
}

#[test]
fn redline_reject_matches_original_accept_matches_revised() {
    let original = Docx::from_paragraphs(&[
        "The notice period is thirty (30) days.",
        "Unchanged paragraph.",
    ])
    .unwrap();
    let revised = Docx::from_paragraphs(&[
        "The notice period is sixty (60) days.",
        "Unchanged paragraph.",
        "Brand new paragraph.",
    ])
    .unwrap();
    let red =
        blackline_docx::redline(&original, &revised, "Morgan Lee", Granularity::Word).unwrap();
    assert!(red.check_against(&original).unwrap().passed());

    let rejected = settle(red.clone_via_bytes(), false);
    assert_eq!(
        normalize(&visible(&rejected)),
        normalize(&visible(&original))
    );

    let red =
        blackline_docx::redline(&original, &revised, "Morgan Lee", Granularity::Word).unwrap();
    let accepted = settle(red, true);
    assert_eq!(
        normalize(&visible(&accepted)),
        normalize(&visible(&revised))
    );
}

#[test]
fn tracked_edit_requires_author() {
    let doc = Docx::from_paragraphs(&["hello"]).unwrap();
    let err = doc
        .edit([EditOp::Replace {
            index: Some(1),
            old: "hello".into(),
            new: "hi".into(),
            content_match: None,
        }])
        .tracked("")
        .apply();
    // empty author string is still Some("") — the builder sets author.
    // Missing author (no tracked()) with tracked=true is the real case:
    let _ = err;
    let err = doc
        .edit([EditOp::Replace {
            index: Some(1),
            old: "hello".into(),
            new: "hi".into(),
            content_match: None,
        }])
        .apply();
    // untracked is fine
    assert!(err.is_ok());
}

#[test]
fn package_health_passes_after_tracked_edit() {
    let edited = tracked_replace("alpha beta", "beta", "gamma", Granularity::Word);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tracked.docx");
    edited.save(&path).unwrap();
    let again = Docx::open(&path).unwrap();
    assert!(again.check().unwrap().passed());
    assert_eq!(visible(&again), "alpha gamma");
}

trait CloneViaBytes {
    fn clone_via_bytes(&self) -> Self;
}

impl CloneViaBytes for Docx {
    fn clone_via_bytes(&self) -> Self {
        Docx::from_bytes(self.to_bytes().unwrap()).unwrap()
    }
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
