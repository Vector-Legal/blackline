//! PPTX create / edit / reopen precision tests.

use blackline_pptx::{CreateSpec, EditOp, EditOptions, Pptx, SlideSpec};

fn apply(p: &mut Pptx, ops: Vec<EditOp>) {
    p.edit(&ops, &EditOptions::default()).unwrap();
}

#[test]
fn create_view_and_find() {
    let p = Pptx::create(&CreateSpec {
        slides: vec![
            SlideSpec {
                texts: vec!["Title".into(), "Subtitle".into()],
                notes: None,
            },
            SlideSpec {
                texts: vec!["Second".into()],
                notes: None,
            },
        ],
    })
    .unwrap();
    assert_eq!(p.slides().unwrap().len(), 2);
    let view = p.view().unwrap();
    assert_eq!(view[0].elements[0], "Title");
    let hits = p.find("Second", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].slide, 2);
    assert!(p.check().unwrap().passed());
}

#[test]
fn set_text_roundtrip() {
    let mut p = Pptx::create(&CreateSpec {
        slides: vec![SlideSpec {
            texts: vec!["Old title".into(), "Body".into()],
            notes: None,
        }],
    })
    .unwrap();
    apply(
        &mut p,
        vec![EditOp::SetText {
            slide: 1,
            element: 1,
            text: "New title".into(),
        }],
    );
    assert_eq!(p.view().unwrap()[0].elements[0], "New title");
    assert_eq!(p.view().unwrap()[0].elements[1], "Body");

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deck.pptx");
    p.save(&path).unwrap();
    let again = Pptx::open(&path).unwrap();
    assert_eq!(again.view().unwrap()[0].elements[0], "New title");
    assert!(again.check().unwrap().passed());
}

#[test]
fn set_text_replaces_every_run_in_the_frame() {
    let mut p = Pptx::create(&CreateSpec {
        slides: vec![SlideSpec {
            texts: vec!["Abc".into()],
            notes: None,
        }],
    })
    .unwrap();
    let part = p.slides().unwrap()[0].part.clone();
    let mut doc = p.package().part_xml(&part).unwrap();
    doc.root.walk_mut(&mut |n| {
        if n.is_element_with_local_name("p") && n.find_child("r").is_some() {
            n.children_mut().push(
                blackline_core::xml::XmlNode::a("r")
                    .with_child(blackline_core::xml::XmlNode::a("t").with_text(" def")),
            );
        }
    });
    p.package_mut().set_part_xml(&part, &doc);
    assert_eq!(p.view().unwrap()[0].elements[0], "Abc def");
    apply(
        &mut p,
        vec![EditOp::SetText {
            slide: 1,
            element: 1,
            text: "BLACKLINE".into(),
        }],
    );
    assert_eq!(p.view().unwrap()[0].elements[0], "BLACKLINE");
    assert!(p.check().unwrap().passed());
}

#[test]
fn insert_and_delete_slide() {
    let mut p = Pptx::create(&CreateSpec {
        slides: vec![SlideSpec {
            texts: vec!["One".into()],
            notes: None,
        }],
    })
    .unwrap();
    apply(
        &mut p,
        vec![EditOp::InsertSlide {
            texts: vec!["Two".into()],
            at: None,
        }],
    );
    assert_eq!(p.slides().unwrap().len(), 2);
    apply(&mut p, vec![EditOp::DeleteSlide { slide: 1 }]);
    assert_eq!(p.slides().unwrap().len(), 1);
    assert_eq!(p.view().unwrap()[0].elements[0], "Two");
}

#[test]
fn insert_slide_at_position() {
    let mut p = Pptx::create(&CreateSpec {
        slides: vec![
            SlideSpec {
                texts: vec!["One".into()],
                notes: None,
            },
            SlideSpec {
                texts: vec!["Three".into()],
                notes: None,
            },
        ],
    })
    .unwrap();
    apply(
        &mut p,
        vec![EditOp::InsertSlide {
            texts: vec!["Two".into()],
            at: Some(2),
        }],
    );
    let view = p.view().unwrap();
    assert_eq!(view.len(), 3);
    assert_eq!(view[0].elements[0], "One");
    assert_eq!(view[1].elements[0], "Two");
    assert_eq!(view[2].elements[0], "Three");
    assert_eq!(p.info().unwrap().slides, 3);
}

#[test]
fn dry_run_does_not_mutate() {
    let mut p = Pptx::create(&CreateSpec {
        slides: vec![SlideSpec {
            texts: vec!["Keep".into()],
            notes: None,
        }],
    })
    .unwrap();
    let report = p
        .edit(
            &[EditOp::SetText {
                slide: 1,
                element: 1,
                text: "Ghost".into(),
            }],
            &EditOptions {
                dry_run: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(report.mode, "dry-run");
    assert_eq!(p.view().unwrap()[0].elements[0], "Keep");
}

#[test]
fn find_respects_limit() {
    let p = Pptx::create(&CreateSpec {
        slides: vec![
            SlideSpec {
                texts: vec!["alpha one".into()],
                notes: None,
            },
            SlideSpec {
                texts: vec!["alpha two".into()],
                notes: None,
            },
        ],
    })
    .unwrap();
    let hits = p.find("alpha", 1).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn cannot_delete_last_slide() {
    let mut p = Pptx::create(&CreateSpec {
        slides: vec![SlideSpec {
            texts: vec!["Only".into()],
            notes: None,
        }],
    })
    .unwrap();
    let err = p.edit(&[EditOp::DeleteSlide { slide: 1 }], &EditOptions::default());
    assert!(err.is_err());
}

#[test]
fn special_characters_on_a_slide() {
    let p = Pptx::create(&CreateSpec {
        slides: vec![SlideSpec {
            texts: vec![r#"A & B < C > "quotes""#.into(), "日本語 café".into()],
            notes: None,
        }],
    })
    .unwrap();
    let els = &p.view().unwrap()[0].elements;
    assert!(els[0].contains('&'));
    assert!(els[1].contains("日本語"));
}
