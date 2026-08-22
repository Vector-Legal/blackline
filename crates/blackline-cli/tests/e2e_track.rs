//! CLI end-to-end tests for `blackline track`.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn bl() -> Command {
    Command::cargo_bin("bl").unwrap()
}

fn fixtures() -> TempDir {
    let dir = TempDir::new().unwrap();
    bl().args(["fixtures", dir.path().to_str().unwrap()])
        .assert()
        .success();
    dir
}

fn path(dir: &TempDir, name: &str) -> String {
    dir.path().join(name).display().to_string()
}

fn corpus_file(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus/docx")
        .join(name);
    assert!(path.is_file(), "missing {}", path.display());
    path
}

fn copy_corpus(dir: &TempDir, name: &str) -> String {
    let dest = dir.path().join(name);
    std::fs::copy(corpus_file(name), &dest).unwrap();
    dest.display().to_string()
}

#[test]
fn apply_replace_insert_delete_comment_then_changes() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    let output = path(&dir, "tracked.docx");
    let ops = r#"[
        {"op":"replace","match":"Hello world.","old":"Hello","new":"Howdy","author":"Jane","date":"2026-02-01T00:00:00Z"},
        {"op":"insert","match":"world","position":"before","text":"wide ","author":"Bob"},
        {"op":"comment","match":"Howdy","anchor":"Howdy","text":"Tone?","author":"Jane","date":"2026-02-01T00:00:00Z"}
    ]"#;

    bl().args([
        "track", "apply", &input, "--ops", ops, "-o", &output, "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"applied\": 3"));

    bl().args(["track", "changes", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"author\": \"Jane\""))
        .stdout(predicate::str::contains("\"author\": \"Bob\""))
        .stdout(predicate::str::contains("2026-02-01T00:00:00Z"));

    bl().args(["track", "changes", &output, "--author", "Bob"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Bob"))
        .stdout(predicate::str::contains("Jane").not());

    bl().args(["track", "comments", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tone?"))
        .stdout(predicate::str::contains("Jane"));

    bl().args(["track", "comments", &output, "--author", "Nobody"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));

    bl().args(["docx", "view", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("Howdy wide world."));

    bl().args(["docx", "check", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));
}

#[test]
fn apply_author_required() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    bl().args([
        "track",
        "apply",
        &input,
        "--ops",
        r#"[{"op":"replace","match":"Hello","old":"Hello","new":"Hi"}]"#,
        "--dry-run",
    ])
    .assert()
    .failure()
    .code(2)
    .stderr(predicate::str::contains("author required"));
}

#[test]
fn apply_uses_blackline_author() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    let output = path(&dir, "env.docx");
    bl().env("BLACKLINE_AUTHOR", "Env Author")
        .args([
            "track",
            "apply",
            &input,
            "--ops",
            r#"[{"op":"replace","match":"Hello","old":"Hello","new":"Hi"}]"#,
            "-o",
            &output,
        ])
        .assert()
        .success();
    bl().args(["track", "changes", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("Env Author"));
}

#[test]
fn apply_dry_run_needs_no_output() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    bl().args([
        "track",
        "apply",
        &input,
        "--ops",
        r#"[{"op":"replace","match":"Hello","old":"Hello","new":"Hi","author":"Jane"}]"#,
        "--dry-run",
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"mode\": \"dry-run\""));
}

#[test]
fn apply_invalid_json_is_usage() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    bl().args(["track", "apply", &input, "--ops", "not-json", "--dry-run"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("invalid --ops"));
}

#[test]
fn settle_requires_accept_or_reject() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    bl().args(["track", "settle", &input, "-o", &path(&dir, "x.docx")])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--accept").or(predicate::str::contains("--reject")));
}

#[test]
fn redline_settle_precision() {
    let dir = fixtures();
    let original = path(&dir, "redline_original.docx");
    let revised = path(&dir, "redline_revised.docx");
    let redlined = path(&dir, "redline.docx");
    let rejected = path(&dir, "rejected.docx");
    let accepted = path(&dir, "accepted.docx");

    bl().args([
        "track",
        "redline",
        &original,
        &revised,
        "-o",
        &redlined,
        "--author",
        "Morgan Lee",
    ])
    .assert()
    .success();

    bl().args(["track", "changes", &redlined])
        .assert()
        .success()
        .stdout(predicate::str::contains("Morgan Lee"));

    bl().args(["docx", "check", &redlined, "--original", &original])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));

    bl().args(["track", "settle", &redlined, "--reject", "-o", &rejected])
        .assert()
        .success();
    bl().args(["docx", "view", &rejected])
        .assert()
        .success()
        .stdout(predicate::str::contains("thirty (30)"))
        .stdout(predicate::str::contains("sixty (60)").not());

    bl().args(["track", "settle", &redlined, "--accept", "-o", &accepted])
        .assert()
        .success();
    bl().args(["docx", "view", &accepted])
        .assert()
        .success()
        .stdout(predicate::str::contains("sixty (60)"));
}

#[test]
fn settle_one_author() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    let tracked = path(&dir, "multi.docx");
    let settled = path(&dir, "settled.docx");
    let ops = r#"[
        {"op":"replace","match":"Hello","old":"Hello","new":"Howdy","author":"Jane"},
        {"op":"replace","match":"world","old":"world","new":"planet","author":"Bob"}
    ]"#;
    bl().args(["track", "apply", &input, "--ops", ops, "-o", &tracked])
        .assert()
        .success();
    bl().args([
        "track", "settle", &tracked, "--accept", "--author", "Jane", "-o", &settled, "--json",
    ])
    .assert()
    .success();
    bl().args(["track", "changes", &settled])
        .assert()
        .success()
        .stdout(predicate::str::contains("Bob"))
        .stdout(predicate::str::contains("Jane").not());
    bl().args(["docx", "view", &settled])
        .assert()
        .success()
        .stdout(predicate::str::contains("Howdy"))
        .stdout(predicate::str::contains("planet"));
}

#[test]
fn surgical_delete_via_cli() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    let output = path(&dir, "deleted.docx");
    bl().args([
        "track",
        "apply",
        &input,
        "--ops",
        r#"[{"op":"delete","match":"Hello world.","text":"world","author":"Jane"}]"#,
        "-o",
        &output,
    ])
    .assert()
    .success();
    bl().args(["track", "changes", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"delete\""))
        .stdout(predicate::str::contains("world"));
    bl().args(["docx", "view", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello"))
        .stdout(predicate::str::contains("world").not());
}

#[test]
fn poi_sample_track_roundtrip() {
    let dir = TempDir::new().unwrap();
    let original = copy_corpus(&dir, "sample.docx");
    let tracked = path(&dir, "tracked.docx");
    let rejected = path(&dir, "rejected.docx");
    let accepted = path(&dir, "accepted.docx");

    bl().args([
        "track",
        "apply",
        &original,
        "--ops",
        r#"[{"op":"replace","match":"Lorem","old":"Lorem","new":"LoremX","author":"Casey Ng"}]"#,
        "-o",
        &tracked,
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &tracked])
        .assert()
        .success()
        .stdout(predicate::str::contains("LoremX"));
    bl().args(["docx", "check", &tracked])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));

    bl().args(["track", "settle", &tracked, "--reject", "-o", &rejected])
        .assert()
        .success();
    bl().args(["docx", "view", &rejected])
        .assert()
        .success()
        .stdout(predicate::str::contains("Lorem"))
        .stdout(predicate::str::contains("LoremX").not());

    bl().args(["track", "settle", &tracked, "--accept", "-o", &accepted])
        .assert()
        .success();
    bl().args(["docx", "view", &accepted])
        .assert()
        .success()
        .stdout(predicate::str::contains("LoremX"));
}

#[test]
fn missing_output_is_usage() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    bl().args([
        "track",
        "apply",
        &input,
        "--ops",
        r#"[{"op":"replace","match":"Hello","old":"Hello","new":"Hi","author":"Jane"}]"#,
    ])
    .assert()
    .failure()
    .code(2)
    .stderr(predicate::str::contains("-o/--output"));
}
