//! CLI end-to-end tests against the vendored Apache POI PPTX corpus.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

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

fn bl() -> Command {
    Command::cargo_bin("bl").unwrap()
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/pptx")
}

fn corpus_file(name: &str) -> PathBuf {
    let path = corpus_dir().join(name);
    assert!(
        path.is_file(),
        "missing corpus file {} (workspace checkout required)",
        path.display()
    );
    path
}

fn copy_to(dir: &TempDir, name: &str) -> String {
    let dest = dir.path().join(name);
    std::fs::copy(corpus_file(name), &dest).unwrap();
    dest.display().to_string()
}

fn out(dir: &TempDir, name: &str) -> String {
    dir.path().join(name).display().to_string()
}

#[test]
fn every_corpus_file_check_view_info_parts() {
    for name in FILES {
        let file = corpus_file(name);
        let file = file.to_str().unwrap();

        bl().args(["pptx", "check", file])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"status\": \"pass\""));

        bl().args(["pptx", "view", file]).assert().success();

        bl().args(["pptx", "info", file])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"slides\""));

        bl().args(["pptx", "parts", file, "--json"])
            .assert()
            .success()
            .stdout(predicate::str::contains("ppt/presentation.xml"));
    }
}

#[test]
fn sample_show_edit_find_and_check() {
    let dir = TempDir::new().unwrap();
    let input = copy_to(&dir, "SampleShow.pptx");
    let output = out(&dir, "edited.pptx");

    bl().args([
        "pptx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"set_text","slide":1,"element":1,"text":"BLACKLINE_CORPUS"}]"#,
        "-o",
        &output,
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"applied\": 1"));

    bl().args(["pptx", "find", &output, "BLACKLINE_CORPUS", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"slide\": 1"));

    bl().args(["pptx", "view", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("BLACKLINE_CORPUS"))
        .stdout(predicate::str::contains("This is the second slide"));

    bl().args(["pptx", "check", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));
}

#[test]
fn japanese_fragments_and_chart_parts() {
    bl().args([
        "pptx",
        "check",
        corpus_file("with_japanese.pptx").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"status\": \"pass\""));

    bl().args([
        "pptx",
        "view",
        corpus_file("with_japanese.pptx").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Here is a text box"));

    bl().args([
        "pptx",
        "parts",
        corpus_file("bar-chart.pptx").to_str().unwrap(),
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("ppt/charts/chart1.xml"));
}

#[test]
fn smartart_audio_comments_and_notes() {
    bl().args([
        "pptx",
        "parts",
        corpus_file("SmartArt.pptx").to_str().unwrap(),
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("ppt/diagrams/data1.xml"));

    bl().args([
        "pptx",
        "parts",
        corpus_file("EmbeddedAudio.pptx").to_str().unwrap(),
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("ppt/media/media1.mp3"));

    bl().args([
        "pptx",
        "parts",
        corpus_file("45545_Comment.pptx").to_str().unwrap(),
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("ppt/comments/comment1.xml"))
    .stdout(predicate::str::contains("ppt/notesSlides/notesSlide1.xml"));
}

#[test]
fn table_and_master_text() {
    bl().args([
        "pptx",
        "view",
        corpus_file("table-with-theme.pptx").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Abc def"));

    bl().args([
        "pptx",
        "view",
        corpus_file("WithMaster.pptx").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("First page title"))
    .stdout(predicate::str::contains("Footer from the master slide"));
}
