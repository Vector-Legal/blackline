//! CLI end-to-end tests against the vendored Apache POI DOCX corpus.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

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

fn bl() -> Command {
    Command::cargo_bin("bl").unwrap()
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/docx")
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

        bl().args(["docx", "check", file])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"status\": \"pass\""));

        bl().args(["docx", "view", file]).assert().success();

        bl().args(["docx", "info", file])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"paragraphs\""));

        bl().args(["docx", "parts", file, "--json"])
            .assert()
            .success()
            .stdout(predicate::str::contains("word/document.xml"));
    }
}

#[test]
fn heading_outline_and_picture_parts() {
    bl().args([
        "docx",
        "outline",
        corpus_file("heading123.docx").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("H1"))
    .stdout(predicate::str::contains("H2"))
    .stdout(predicate::str::contains("H3"));

    bl().args([
        "docx",
        "parts",
        corpus_file("VariousPictures.docx").to_str().unwrap(),
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("word/media/image5.jpeg"))
    .stdout(predicate::str::contains("word/media/image2.png"));
}

#[test]
fn comments_on_real_word_file() {
    bl().args([
        "docx",
        "comments",
        corpus_file("testComment.docx").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"author\": \"poi\""))
    .stdout(predicate::str::contains("comment content"));
}

#[test]
fn delins_lists_existing_revisions() {
    bl().args([
        "docx",
        "changes",
        corpus_file("delins.docx").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"author\": \"pavel\""))
    .stdout(predicate::str::contains("\"kind\": \"insert\""))
    .stdout(predicate::str::contains("\"kind\": \"delete\""));
}

fn redline_replace_roundtrip(name: &str, needle: &str) {
    let dir = TempDir::new().unwrap();
    let original = copy_to(&dir, name);
    let revised = out(&dir, "revised.docx");
    let redlined = out(&dir, "redline.docx");
    let rejected = out(&dir, "rejected.docx");
    let accepted = out(&dir, "accepted.docx");
    let replacement = format!("{needle}X");
    let ops = format!(
        r#"[{{"op":"replace","match":"{needle}","old":"{needle}","new":"{replacement}"}}]"#
    );

    bl().args(["docx", "edit", &original, "--ops", &ops, "-o", &revised])
        .assert()
        .success();
    bl().args(["docx", "view", &revised])
        .assert()
        .success()
        .stdout(predicate::str::contains(&replacement));

    bl().args([
        "docx", "redline", &original, &revised, "-o", &redlined, "--author", "Casey Ng",
    ])
    .assert()
    .success();

    bl().args(["docx", "changes", &redlined])
        .assert()
        .success()
        .stdout(predicate::str::contains("Casey Ng"));

    bl().args(["docx", "check", &redlined, "--original", &original])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));

    bl().args([
        "docx",
        "edit",
        &redlined,
        "--ops",
        r#"[{"op":"reject_all"}]"#,
        "-o",
        &rejected,
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &rejected])
        .assert()
        .success()
        .stdout(predicate::str::contains(needle))
        .stdout(predicate::str::contains(&replacement).not());

    bl().args([
        "docx",
        "edit",
        &redlined,
        "--ops",
        r#"[{"op":"accept_all"}]"#,
        "-o",
        &accepted,
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &accepted])
        .assert()
        .success()
        .stdout(predicate::str::contains(&replacement));

    bl().args(["docx", "check", &rejected]).assert().success();
    bl().args(["docx", "check", &accepted]).assert().success();
}

#[test]
fn redline_replace_on_rich_poi_files() {
    redline_replace_roundtrip("sample.docx", "Lorem");
    redline_replace_roundtrip("TestDocument.docx", "test");
    redline_replace_roundtrip("heading123.docx", "First");
    redline_replace_roundtrip("HeaderFooterUnicode.docx", "fairly");
    redline_replace_roundtrip("footnotes.docx", "text");
    redline_replace_roundtrip("Numbering.docx", "Level");
}

#[test]
fn tracked_edit_accept_reject_on_sample() {
    let dir = TempDir::new().unwrap();
    let original = copy_to(&dir, "sample.docx");
    let tracked = out(&dir, "tracked.docx");
    let rejected = out(&dir, "rejected.docx");
    let accepted = out(&dir, "accepted.docx");

    bl().env("BLACKLINE_AUTHOR", "Casey Ng")
        .args([
            "docx",
            "edit",
            &original,
            "--ops",
            r#"[{"op":"replace","match":"Lorem","old":"Lorem","new":"LoremX"}]"#,
            "--track",
            "--granularity",
            "word",
            "-o",
            &tracked,
        ])
        .assert()
        .success();

    bl().args(["docx", "changes", &tracked])
        .assert()
        .success()
        .stdout(predicate::str::contains("Casey Ng"));

    bl().args(["docx", "check", &tracked, "--original", &original])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));

    bl().args([
        "docx",
        "edit",
        &tracked,
        "--ops",
        r#"[{"op":"reject_all"}]"#,
        "-o",
        &rejected,
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &rejected])
        .assert()
        .success()
        .stdout(predicate::str::contains("Lorem"))
        .stdout(predicate::str::contains("LoremX").not());

    bl().args([
        "docx",
        "edit",
        &tracked,
        "--ops",
        r#"[{"op":"accept_all"}]"#,
        "-o",
        &accepted,
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &accepted])
        .assert()
        .success()
        .stdout(predicate::str::contains("LoremX"));
}

#[test]
fn delins_flatten_then_redline() {
    let dir = TempDir::new().unwrap();
    let source = copy_to(&dir, "delins.docx");
    let flat = out(&dir, "flat.docx");
    let revised = out(&dir, "revised.docx");
    let redlined = out(&dir, "redline.docx");
    let rejected = out(&dir, "rejected.docx");

    bl().args([
        "docx",
        "edit",
        &source,
        "--ops",
        r#"[{"op":"accept_all"}]"#,
        "-o",
        &flat,
    ])
    .assert()
    .success();

    bl().args(["docx", "changes", &flat])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));

    bl().args([
        "docx",
        "edit",
        &flat,
        "--ops",
        r#"[{"op":"replace","match":"Tika","old":"Tika","new":"TikaX"}]"#,
        "-o",
        &revised,
    ])
    .assert()
    .success();

    bl().args([
        "docx", "redline", &flat, &revised, "-o", &redlined, "--author", "Casey Ng",
    ])
    .assert()
    .success();

    bl().args(["docx", "check", &redlined, "--original", &flat])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));

    bl().args([
        "docx",
        "edit",
        &redlined,
        "--ops",
        r#"[{"op":"reject_all"}]"#,
        "-o",
        &rejected,
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &rejected])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tika"))
        .stdout(predicate::str::contains("TikaX").not());
}

#[test]
fn table_insert_then_redline_new_paragraph() {
    let dir = TempDir::new().unwrap();
    let original = copy_to(&dir, "TestTableCellAlign.docx");
    let revised = out(&dir, "revised.docx");
    let redlined = out(&dir, "redline.docx");
    let rejected = out(&dir, "rejected.docx");

    bl().args([
        "docx",
        "edit",
        &original,
        "--ops",
        r#"[{"op":"insert","match":"Top","position":"after","text":"BLACKLINE_INSERT"}]"#,
        "-o",
        &revised,
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &revised])
        .assert()
        .success()
        .stdout(predicate::str::contains("BLACKLINE_INSERT"));

    bl().args([
        "docx", "redline", &original, &revised, "-o", &redlined, "--author", "Casey Ng",
    ])
    .assert()
    .success();

    bl().args(["docx", "check", &redlined, "--original", &original])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));

    bl().args([
        "docx",
        "edit",
        &redlined,
        "--ops",
        r#"[{"op":"reject_all"}]"#,
        "-o",
        &rejected,
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &rejected])
        .assert()
        .success()
        .stdout(predicate::str::contains("Top"))
        .stdout(predicate::str::contains("BLACKLINE_INSERT").not());
}

#[test]
fn comment_file_insert_keeps_existing_thread() {
    let dir = TempDir::new().unwrap();
    let original = copy_to(&dir, "testComment.docx");
    let revised = out(&dir, "revised.docx");
    let redlined = out(&dir, "redline.docx");

    bl().args([
        "docx",
        "edit",
        &original,
        "--ops",
        r#"[{"op":"insert","index":1,"position":"after","text":"BLACKLINE_INSERT"}]"#,
        "-o",
        &revised,
    ])
    .assert()
    .success();

    bl().args(["docx", "comments", &revised])
        .assert()
        .success()
        .stdout(predicate::str::contains("comment content"));

    bl().args([
        "docx", "redline", &original, &revised, "-o", &redlined, "--author", "Casey Ng",
    ])
    .assert()
    .success();
    bl().args(["docx", "check", &redlined, "--original", &original])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));
}

#[test]
fn header_edit_and_hyperlink_ops() {
    let dir = TempDir::new().unwrap();
    let original = copy_to(&dir, "HeaderFooterUnicode.docx");
    let headered = out(&dir, "header.docx");
    bl().args([
        "docx",
        "edit",
        &original,
        "--part",
        "header",
        "--ops",
        r#"[{"op":"replace","match":"euro","old":"euro","new":"EUR"}]"#,
        "-o",
        &headered,
    ])
    .assert()
    .success();
    bl().args(["docx", "cat", &headered, "word/header2.xml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("EUR"));

    let linked_in = copy_to(&dir, "TestDocument.docx");
    let linked = out(&dir, "linked.docx");
    bl().args([
        "docx",
        "edit",
        &linked_in,
        "--ops",
        r#"[{"op":"set_hyperlink","match":"normal","text":"normal","url":"mailto:docket@example.com"}]"#,
        "-o",
        &linked,
    ])
    .assert()
    .success();
    bl().args(["docx", "cat", &linked, "word/document.xml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("w:hyperlink"));
    bl().args(["docx", "cat", &linked, "word/_rels/document.xml.rels"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mailto:docket@example.com"));
}
