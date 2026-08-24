//! CLI surface without compiling Kalosm: help, usage, refused file types.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

use blackline_docx::Docx;

fn bin() -> Command {
    Command::cargo_bin("blackline-llm").unwrap()
}

#[test]
fn help_names_the_default_model() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("phi-3.5"))
        .stdout(predicate::str::contains("--model"))
        .stdout(predicate::str::contains("--author"));
}

#[test]
fn pdf_is_refused() {
    let dir = TempDir::new().unwrap();
    let pdf = dir.path().join("memo.pdf");
    std::fs::write(&pdf, b"%PDF").unwrap();
    bin()
        .args([pdf.to_str().unwrap(), "summarize this", "--dry-run"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains(".docx"));
}

#[test]
fn markdown_is_refused() {
    let dir = TempDir::new().unwrap();
    let md = dir.path().join("notes.md");
    std::fs::write(&md, "# Hi\n").unwrap();
    bin()
        .args([md.to_str().unwrap(), "edit this", "--dry-run"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("does not convert"));
}

#[test]
fn docx_redline_requires_author() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.docx");
    Docx::from_paragraphs(&["Hello"])
        .unwrap()
        .save(&input)
        .unwrap();
    bin()
        .args([input.to_str().unwrap(), "change hello to hi", "--dry-run"])
        .env_remove("BLACKLINE_AUTHOR")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("author required"));
}

#[test]
fn missing_output_is_usage() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.docx");
    Docx::from_paragraphs(&["Hello"])
        .unwrap()
        .save(&input)
        .unwrap();
    bin()
        .args([
            input.to_str().unwrap(),
            "change hello to hi",
            "--author",
            "Jane",
        ])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("-o/--output"));
}

#[test]
fn unknown_model_is_usage() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.docx");
    Docx::from_paragraphs(&["Hello"])
        .unwrap()
        .save(&input)
        .unwrap();
    bin()
        .args([
            input.to_str().unwrap(),
            "change hello to hi",
            "--dry-run",
            "--author",
            "Jane",
            "--model",
            "gpt-4",
        ])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("phi-3.5"));
}

#[test]
fn without_kalosm_feature_run_explains_install() {
    // Default CI builds omit `kalosm`. The binary must still start and say so.
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.docx");
    Docx::from_paragraphs(&["Hello"])
        .unwrap()
        .save(&input)
        .unwrap();
    let assert = bin()
        .args([
            input.to_str().unwrap(),
            "change hello to hi",
            "--dry-run",
            "--author",
            "Jane",
        ])
        .assert();
    if cfg!(feature = "kalosm") {
        // A real model run is not part of CI (download + minutes).
        return;
    }
    assert
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--features kalosm"));
}
