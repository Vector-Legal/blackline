//! End-to-end open / view / edit against vendored Apache POI PPTX files.
//!
//! PPTX has no tracked-change / redline API. Precision here means: after a
//! surgical `set_text` on an existing `txBody`, `check` passes, the new
//! text is visible, and sidecar parts (media, charts, diagrams, notes,
//! masters, comments) keep their original bytes.
//!
//! `insert_slide` / `delete_slide` rebuild the package from a text spec and
//! are not used against these real-world files.

use std::path::PathBuf;

use blackline_pptx::{EditOp, EditOptions, Pptx};

const FILES: &[&str] = &[
    "SampleShow.pptx",
    "sample.pptx",
    "present1.pptx",
    "WithMaster.pptx",
    "SmartArt.pptx",
    "table_test.pptx",
    "table-with-theme.pptx",
    "bar-chart.pptx",
    "pie-chart.pptx",
    "with_japanese.pptx",
    "shapes.pptx",
    "layouts.pptx",
    "bug58144-headers-footers-2007.pptx",
    "45545_Comment.pptx",
    "EmbeddedAudio.pptx",
];

const MARKER: &str = "BLACKLINE_CORPUS";

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/pptx")
}

fn corpus_path(name: &str) -> PathBuf {
    corpus_dir().join(name)
}

fn open(name: &str) -> Pptx {
    let path = corpus_path(name);
    assert!(
        path.is_file(),
        "missing corpus file {} (workspace checkout required)",
        path.display()
    );
    Pptx::open(&path).unwrap_or_else(|e| panic!("open {name}: {e}"))
}

fn apply(p: &mut Pptx, ops: Vec<EditOp>) {
    p.edit(&ops, &EditOptions::default())
        .unwrap_or_else(|e| panic!("edit failed: {e}"));
}

fn assert_check(p: &Pptx, label: &str) {
    let health = p.check().unwrap();
    assert!(
        health.passed(),
        "{label}: check failed: {}",
        serde_json::to_string_pretty(&health).unwrap_or_else(|_| format!("{health:?}"))
    );
}

fn clone_deck(p: &Pptx) -> Pptx {
    Pptx::from_bytes(p.to_bytes().unwrap()).unwrap()
}

/// First (slide, 1-based element) that already has a `txBody`.
fn first_text_target(p: &Pptx) -> Option<(usize, usize)> {
    for slide in p.view().unwrap() {
        if !slide.elements.is_empty() {
            return Some((slide.index, 1));
        }
    }
    None
}

fn is_sidecar(name: &str) -> bool {
    name.contains("/media/")
        || name.contains("/charts/")
        || name.contains("/diagrams/")
        || name.contains("/theme/")
        || name.contains("/notesSlides/")
        || name.contains("/notesMasters/")
        || name.contains("/slideMasters/")
        || name.contains("/slideLayouts/")
        || name.contains("/comments/")
        || name.contains("commentAuthors")
        || name.ends_with(".mp3")
        || name.ends_with(".jpeg")
        || name.ends_with(".jpg")
        || name.ends_with(".png")
        || name.ends_with(".wmf")
        || name.ends_with(".emf")
}

fn assert_sidecar_parts_untouched(before: &Pptx, after: &Pptx, label: &str) {
    for (name, bytes) in before.package().iter() {
        if !is_sidecar(name) {
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
        .filter(|n| n.ends_with(".pptx") && !FILES.contains(&n.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "unexpected extra corpus files (update FILES): {extra:?}"
    );
}

#[test]
fn every_corpus_file_opens_views_and_checks() {
    for name in FILES {
        let p = open(name);
        let slides = p.slides().unwrap();
        assert!(!slides.is_empty(), "{name}: no slides");
        let view = p.view().unwrap();
        assert_eq!(
            view.len(),
            slides.len(),
            "{name}: view/slide count mismatch"
        );
        assert_check(&p, name);
        let info = p.info().unwrap();
        assert_eq!(info.slides, slides.len(), "{name}: info.slides");
        let _ = p.find("a", 5).unwrap();
    }
}

#[test]
fn every_file_set_text_or_roundtrip() {
    for name in FILES {
        let original = open(name);
        let Some((slide, element)) = first_text_target(&original) else {
            // SmartArt / embedded audio: text lives in diagrams or media,
            // not slide txBody. Assert lossless save/reopen instead.
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(name);
            original.save(&path).unwrap();
            let again = Pptx::open(&path).unwrap();
            assert_check(&again, &format!("{name} reopen"));
            assert_eq!(
                again.slides().unwrap().len(),
                original.slides().unwrap().len(),
                "{name}: slide count after reopen"
            );
            assert_sidecar_parts_untouched(&original, &again, name);
            continue;
        };

        let mut revised = clone_deck(&original);
        apply(
            &mut revised,
            vec![EditOp::SetText {
                slide,
                element,
                text: MARKER.into(),
            }],
        );
        assert_check(&revised, &format!("{name} set_text"));
        let view = revised.view().unwrap();
        let slide_view = view
            .iter()
            .find(|s| s.index == slide)
            .unwrap_or_else(|| panic!("{name}: slide {slide} missing after edit"));
        assert_eq!(
            slide_view.elements[element - 1],
            MARKER,
            "{name}: element {element} was not set"
        );
        let hits = revised.find(MARKER, 10).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.slide == slide && h.element == element),
            "{name}: find missed slide {slide} element {element}"
        );
        assert_sidecar_parts_untouched(&original, &revised, name);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        revised.save(&path).unwrap();
        let again = Pptx::open(&path).unwrap();
        assert_check(&again, &format!("{name} reopen"));
        assert_eq!(
            again.view().unwrap()[slide - 1].elements[element - 1],
            MARKER
        );
        assert_sidecar_parts_untouched(&original, &again, &format!("{name} reopen"));
    }
}

#[test]
fn japanese_footnote_fragments_pass_check() {
    let p = open("with_japanese.pptx");
    assert_check(&p, "with_japanese");
    let view = p.view().unwrap();
    let joined: String = view
        .iter()
        .flat_map(|s| s.elements.iter())
        .cloned()
        .collect();
    assert!(
        joined.contains("日本語") || joined.contains("ゾルゲ") || joined.contains("Gothic"),
        "expected Japanese or Gothic text, got {joined:?}"
    );
    let rels = String::from_utf8_lossy(
        p.package()
            .part("ppt/slides/_rels/slide1.xml.rels")
            .unwrap(),
    );
    assert!(
        rels.contains("#_ftn1"),
        "fixture should keep fragment hyperlink targets"
    );
}

#[test]
fn charts_notes_comments_and_audio_are_sidecars() {
    let bar = open("bar-chart.pptx");
    assert!(bar.package().has_part("ppt/charts/chart1.xml"));
    let mut edited = clone_deck(&bar);
    apply(
        &mut edited,
        vec![EditOp::SetText {
            slide: 1,
            element: 1,
            text: MARKER.into(),
        }],
    );
    assert_sidecar_parts_untouched(&bar, &edited, "bar-chart");

    let commented = open("45545_Comment.pptx");
    assert!(commented
        .package()
        .part_names()
        .iter()
        .any(|n| n.contains("/comments/")));
    assert!(commented
        .package()
        .part_names()
        .iter()
        .any(|n| n.contains("/notesSlides/")));

    let audio = open("EmbeddedAudio.pptx");
    assert!(audio.package().has_part("ppt/media/media1.mp3"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audio.pptx");
    audio.save(&path).unwrap();
    let again = Pptx::open(&path).unwrap();
    assert_sidecar_parts_untouched(&audio, &again, "EmbeddedAudio");
}

#[test]
fn smartart_diagrams_survive_reopen() {
    let p = open("SmartArt.pptx");
    assert!(p.package().has_part("ppt/diagrams/data1.xml"));
    assert!(
        first_text_target(&p).is_none(),
        "SmartArt text lives in diagram parts, not slide txBody"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("smart.pptx");
    p.save(&path).unwrap();
    let again = Pptx::open(&path).unwrap();
    assert_check(&again, "SmartArt reopen");
    assert_sidecar_parts_untouched(&p, &again, "SmartArt");
}

#[test]
fn sample_show_keeps_second_slide() {
    let mut p = open("SampleShow.pptx");
    let second = p.view().unwrap()[1].elements.clone();
    apply(
        &mut p,
        vec![EditOp::SetText {
            slide: 1,
            element: 1,
            text: MARKER.into(),
        }],
    );
    assert_eq!(p.view().unwrap()[1].elements, second);
    assert_eq!(p.view().unwrap()[0].elements[0], MARKER);
}
