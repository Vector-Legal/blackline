//! Real-world DOCX stress tests: multi-author markup, hyperlinks, headers,
//! emails/linkouts, and formatting that the first corpus layer did not assert.

use std::path::PathBuf;
use std::thread;

use blackline_docx::{Docx, EditOp, Granularity, ParaProps};

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus/docx")
        .join(name)
}

fn open(name: &str) -> Docx {
    Docx::open(corpus(name)).unwrap()
}

fn apply(doc: Docx, ops: Vec<EditOp>) -> Docx {
    doc.edit(ops)
        .apply()
        .unwrap()
        .document
        .expect("edit produced a document")
}

fn tracked(doc: Docx, ops: Vec<EditOp>, author: &str) -> Docx {
    doc.edit(ops)
        .tracked(author)
        .granularity(Granularity::Word)
        .apply()
        .unwrap()
        .document
        .expect("tracked edit produced a document")
}

fn replace(old: &str, new: &str) -> EditOp {
    EditOp::Replace {
        index: None,
        old: old.into(),
        new: new.into(),
        content_match: Some(old.into()),
    }
}

fn xml(doc: &Docx, part: &str) -> String {
    doc.package().part_xml(part).unwrap().root.to_xml_string()
}

fn clone_doc(doc: &Docx) -> Docx {
    Docx::from_bytes(doc.to_bytes().unwrap()).unwrap()
}

#[test]
fn replace_keeps_hyperlink_wrapper_and_url() {
    let original = open("TestDocument.docx");
    let links = original.hyperlinks().unwrap();
    assert!(
        links
            .iter()
            .any(|l| l.target.contains("poi.apache.org") && l.text.contains("hyperlink")),
        "POI TestDocument should have the apache.org hyperlink: {links:?}"
    );

    let edited = apply(original, vec![replace("We have a", "We still have a")]);
    assert!(edited.visible_text().unwrap().contains("We still have a"));
    assert!(edited.check().unwrap().passed());

    let links = edited.hyperlinks().unwrap();
    assert_eq!(links.len(), 1, "hyperlink was destroyed: {links:?}");
    assert_eq!(links[0].target, "http://poi.apache.org/");
    assert!(links[0].text.contains("hyperlink"));
    assert!(xml(&edited, "word/document.xml").contains("<w:hyperlink"));
}

#[test]
fn replace_hyperlink_display_text_keeps_target() {
    let edited = apply(
        open("TestDocument.docx"),
        vec![replace("hyperlink", "linkout")],
    );
    let links = edited.hyperlinks().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "http://poi.apache.org/");
    assert!(links[0].text.contains("linkout"));
    assert!(edited.visible_text().unwrap().contains("linkout"));
}

#[test]
fn set_mailto_and_https_linkouts() {
    let doc = apply(
        open("TestDocument.docx"),
        vec![
            EditOp::SetHyperlink {
                index: Some(3),
                content_match: None,
                text: "normal".into(),
                url: "mailto:counsel@example.com".into(),
            },
            EditOp::SetHyperlink {
                index: Some(1),
                content_match: None,
                text: "test".into(),
                url: "https://example.com/exhibits/a".into(),
            },
        ],
    );
    let links = doc.hyperlinks().unwrap();
    assert!(
        links
            .iter()
            .any(|l| l.target == "mailto:counsel@example.com"),
        "missing mailto: {links:?}"
    );
    assert!(
        links
            .iter()
            .any(|l| l.target == "https://example.com/exhibits/a"),
        "missing https linkout: {links:?}"
    );
    assert!(
        links.iter().any(|l| l.target == "http://poi.apache.org/"),
        "original hyperlink must survive: {links:?}"
    );
    assert!(doc.check().unwrap().passed());
}

#[test]
fn retarget_existing_hyperlink() {
    let doc = apply(
        open("TestDocument.docx"),
        vec![EditOp::SetHyperlink {
            index: None,
            content_match: Some("hyperlink".into()),
            text: "hyperlink".into(),
            url: "https://vector.legal/memo".into(),
        }],
    );
    let links = doc.hyperlinks().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "https://vector.legal/memo");
    assert!(links[0].text.contains("hyperlink"));
}

#[test]
fn two_authors_tracked_edits_coexist_and_filter() {
    let original = open("TestDocument.docx");
    let after_alice = tracked(
        clone_doc(&original),
        vec![replace("This", "That")],
        "Alice Chen",
    );
    let after_bob = tracked(after_alice, vec![replace("document", "file")], "Bob Ortiz");

    let changes = after_bob.changes().unwrap();
    assert!(
        changes.iter().any(|c| c.author == "Alice Chen"),
        "Alice's markup was wiped: {changes:?}"
    );
    assert!(
        changes.iter().any(|c| c.author == "Bob Ortiz"),
        "Bob's markup missing: {changes:?}"
    );
    assert!(after_bob.visible_text().unwrap().contains("That"));
    assert!(after_bob.visible_text().unwrap().contains("file"));
    assert!(
        after_bob
            .hyperlinks()
            .unwrap()
            .iter()
            .any(|l| l.target.contains("poi.apache.org")),
        "later author must not destroy the hyperlink"
    );
    assert!(after_bob.check().unwrap().passed());

    let alice_rejected = apply(
        clone_doc(&after_bob),
        vec![EditOp::RejectAll {
            author: Some("Alice Chen".into()),
        }],
    );
    assert!(alice_rejected.visible_text().unwrap().contains("This"));
    assert!(alice_rejected.visible_text().unwrap().contains("file"));
    assert!(alice_rejected
        .changes()
        .unwrap()
        .iter()
        .all(|c| c.author != "Alice Chen"));

    let bob_accepted = apply(
        after_bob,
        vec![EditOp::AcceptAll {
            author: Some("Bob Ortiz".into()),
        }],
    );
    assert!(bob_accepted.visible_text().unwrap().contains("file"));
    assert!(bob_accepted.check().unwrap().passed());
}

#[test]
fn parallel_clones_edit_independently() {
    let bytes = open("sample.docx").to_bytes().unwrap();
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let bytes = bytes.clone();
            thread::spawn(move || {
                let doc = Docx::from_bytes(bytes).unwrap();
                let edited = apply(doc, vec![replace("Lorem", &format!("Lorem{i}"))]);
                assert!(edited
                    .visible_text()
                    .unwrap()
                    .contains(&format!("Lorem{i}")));
                assert!(edited.check().unwrap().passed());
            })
        })
        .collect();
    for h in handles {
        h.join().expect("worker thread panicked");
    }
}

#[test]
fn header_and_footer_text_can_be_edited() {
    let original = open("HeaderFooterUnicode.docx");
    let parts = original.story_parts();
    assert!(parts.iter().any(|p| p.contains("header2")));
    let header_view = original.story_view("word/header2.xml").unwrap();
    assert!(
        header_view.iter().any(|l| l.text.contains("euro")),
        "header2 should mention euro: {header_view:?}"
    );

    let edited = original
        .edit([replace("euro", "EUR")])
        .part("header")
        .apply()
        .unwrap()
        .document
        .unwrap();
    let header_text: String = edited
        .story_view("word/header2.xml")
        .unwrap()
        .into_iter()
        .map(|l| l.text)
        .collect();
    assert!(
        header_text.contains("EUR"),
        "header not edited: {header_text}"
    );
    assert!(
        edited.visible_text().unwrap().contains("fairly simple"),
        "body must stay intact when editing a header"
    );
    assert!(edited.check().unwrap().passed());

    let footered = edited
        .edit([replace("footer", "colophon")])
        .part("footer")
        .apply()
        .unwrap()
        .document
        .unwrap();
    let footer_text: String = footered
        .story_view("word/footer2.xml")
        .unwrap()
        .into_iter()
        .map(|l| l.text)
        .collect();
    assert!(
        footer_text.contains("colophon"),
        "footer not edited: {footer_text}"
    );
    assert!(footered.check().unwrap().passed());
}

#[test]
fn bookmarks_survive_nearby_replace() {
    let edited = apply(open("bookmarks.docx"), vec![replace("Sample", "Example")]);
    let xml = xml(&edited, "word/document.xml");
    assert!(xml.contains("w:bookmarkStart"), "bookmarkStart removed");
    assert!(xml.contains("w:bookmarkEnd"), "bookmarkEnd removed");
    assert!(xml.contains("w:name=\"poi\"") || xml.contains("w:name=\"xwpf\""));
    assert!(edited.visible_text().unwrap().contains("Example"));
    assert!(edited.check().unwrap().passed());
}

#[test]
fn multi_run_formatting_survives_unrelated_replace() {
    let original = open("TestDocument.docx");
    let before = xml(&original, "word/document.xml");
    assert!(before.contains("YELLOW"));
    let edited = apply(
        original,
        vec![replace("This contains", "This still contains")],
    );
    let after = xml(&edited, "word/document.xml");
    assert!(after.contains("YELLOW"), "colored run was collapsed");
    assert!(after.contains("BOLD"), "bold run was collapsed");
    assert!(edited
        .visible_text()
        .unwrap()
        .contains("This still contains"));
    assert!(edited.check().unwrap().passed());
}

#[test]
fn comments_survive_insert_and_check_catches_orphans() {
    let original = open("testComment.docx");
    assert_eq!(original.comments().unwrap().len(), 1);
    assert!(original.check().unwrap().passed());

    let edited = apply(
        original,
        vec![EditOp::Insert {
            index: Some(1),
            position: "after".into(),
            content_match: None,
            text: Some("A later paragraph.".into()),
            content: None,
            style: None,
            para_props: ParaProps::default(),
        }],
    );
    assert_eq!(edited.comments().unwrap().len(), 1);
    assert!(xml(&edited, "word/document.xml").contains("commentRangeStart"));
    assert!(edited.check().unwrap().passed());
}

#[test]
fn redline_preserves_hyperlink_on_nearby_edit() {
    let original = open("TestDocument.docx");
    let revised = apply(
        clone_doc(&original),
        vec![replace("We have a", "We hold a")],
    );
    let red =
        blackline_docx::redline(&original, &revised, "Morgan Lee", Granularity::Word).unwrap();
    assert!(red.check_against(&original).unwrap().passed());
    assert!(
        red.hyperlinks()
            .unwrap()
            .iter()
            .any(|l| l.target.contains("poi.apache.org")),
        "redline dropped the hyperlink"
    );
    let rejected = apply(clone_doc(&red), vec![EditOp::RejectAll { author: None }]);
    assert!(rejected.visible_text().unwrap().contains("We have a"));
}

#[test]
fn table_cell_replace_does_not_destroy_grid() {
    let original = open("TestTableCellAlign.docx");
    let edited = apply(original, vec![replace("Center", "Middle")]);
    assert!(edited.visible_text().unwrap().contains("Middle"));
    assert!(edited.info().unwrap().tables == 1);
    let xml = xml(&edited, "word/document.xml");
    assert!(xml.contains("<w:tbl"), "table element gone");
    assert!(xml.contains("<w:tc"), "table cells gone");
    assert!(edited.check().unwrap().passed());
}

#[test]
fn format_then_replace_then_link() {
    let doc = apply(
        open("TestDocument.docx"),
        vec![EditOp::Format {
            index: 3,
            run_props: blackline_docx::RunProps {
                bold: Some(true),
                color: Some("003366".into()),
                ..Default::default()
            },
            para_props: ParaProps::default(),
        }],
    );
    let doc = apply(doc, vec![replace("Back to", "Return to")]);
    let doc = apply(
        doc,
        vec![EditOp::SetHyperlink {
            index: Some(3),
            content_match: None,
            text: "normal".into(),
            url: "mailto:docket@example.com".into(),
        }],
    );
    assert!(doc.visible_text().unwrap().contains("Return to"));
    assert!(doc
        .hyperlinks()
        .unwrap()
        .iter()
        .any(|l| l.target == "mailto:docket@example.com"));
    assert!(xml(&doc, "word/document.xml").contains("003366"));
    assert!(doc.check().unwrap().passed());
}
