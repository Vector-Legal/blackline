//! CLI end-to-end tests against the vendored Apache POI XLSX corpus.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const FILES: &[&str] = &[
    "SampleSS.xlsx",
    "Formatting.xlsx",
    "InlineString.xlsx",
    "SimpleWithComments.xlsx",
    "shared_formulas.xlsx",
    "formula-eval.xlsx",
    "WithTable.xlsx",
    "WithChart.xlsx",
    "WithConditionalFormatting.xlsx",
    "headerFooterTest.xlsx",
    "TwoSheetsNoneHidden.xlsx",
    "unicodeSheetName.xlsx",
    "sharedhyperlink.xlsx",
    "ForShifting.xlsx",
    "54084 - Greek - beyond BMP.xlsx",
];

fn bl() -> Command {
    Command::cargo_bin("bl").unwrap()
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/xlsx")
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

        bl().args(["xlsx", "check", file])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"status\": \"pass\""));

        bl().args(["xlsx", "view", file]).assert().success();

        bl().args(["xlsx", "info", file])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"sheets\""));

        bl().args(["xlsx", "parts", file, "--json"])
            .assert()
            .success()
            .stdout(predicate::str::contains("xl/workbook.xml"));
    }
}

#[test]
fn sample_edit_find_and_check() {
    let dir = TempDir::new().unwrap();
    let input = copy_to(&dir, "SampleSS.xlsx");
    let output = out(&dir, "edited.xlsx");

    bl().args([
        "xlsx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"set_cell","sheet":"First Sheet","cell":"Z99","value":987654321}]"#,
        "-o",
        &output,
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"applied\": 1"));

    bl().args(["xlsx", "find", &output, "987654321", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Z99"));

    bl().args(["xlsx", "check", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));

    bl().args(["xlsx", "view", &output, "--sheet", "Sheet Number 2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Start of 2nd sheet"));
}

#[test]
fn inline_string_and_unicode_sheet() {
    bl().args([
        "xlsx",
        "view",
        corpus_file("InlineString.xlsx").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("A1\t"))
    .stdout(predicate::str::contains("more text"));

    let dir = TempDir::new().unwrap();
    let input = copy_to(&dir, "unicodeSheetName.xlsx");
    let output = out(&dir, "unicode.xlsx");
    bl().args([
        "xlsx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"set_cell","sheet":"Sheet・1","cell":"B1","value":"MARK"}]"#,
        "-o",
        &output,
    ])
    .assert()
    .success();
    bl().args(["xlsx", "find", &output, "MARK", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sheet・1"));
}

#[test]
fn chart_and_comments_parts() {
    bl().args([
        "xlsx",
        "parts",
        corpus_file("WithChart.xlsx").to_str().unwrap(),
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("xl/charts/chart1.xml"));

    bl().args([
        "xlsx",
        "parts",
        corpus_file("SimpleWithComments.xlsx").to_str().unwrap(),
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("xl/comments1.xml"));
}

#[test]
fn two_sheets_and_hyperlinks() {
    bl().args([
        "xlsx",
        "view",
        corpus_file("TwoSheetsNoneHidden.xlsx").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Sheet1A1"))
    .stdout(predicate::str::contains("Sheet2A1"));

    bl().args([
        "xlsx",
        "find",
        corpus_file("sharedhyperlink.xlsx").to_str().unwrap(),
        "apache.org",
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Hoja1"));
}
