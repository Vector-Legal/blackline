//! Agent-facing CLI: verbs, JSON, exit codes, author policy.

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

#[test]
fn fixtures_writes_office_files() {
    let dir = fixtures();
    for name in [
        "simple.docx",
        "tracked.docx",
        "comments.docx",
        "table.docx",
        "simple.xlsx",
        "formulas.xlsx",
        "simple.pptx",
        "multi_slide.pptx",
    ] {
        assert!(dir.path().join(name).exists(), "missing {name}");
    }
}

#[test]
fn docx_view_info_find_outline() {
    let dir = fixtures();
    let file = path(&dir, "headings.docx");

    bl().args(["docx", "view", &file])
        .assert()
        .success()
        .stdout(predicate::str::contains("Chapter One"));

    bl().args(["docx", "view", &file, "--json", "--from", "1", "--to", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"index\""));

    bl().args(["docx", "outline", &file])
        .assert()
        .success()
        .stdout(predicate::str::contains("H1"));

    bl().args(["docx", "info", &file])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"paragraphs\""));

    bl().args(["docx", "find", &file, "Chapter", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"matches\""));
}

#[test]
fn docx_edit_plain_and_reopen() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    let output = path(&dir, "out.docx");
    bl().args([
        "docx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"replace","index":1,"old":"Hello world.","new":"Goodbye world."}]"#,
        "-o",
        &output,
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"applied\": 1"));

    bl().args(["docx", "view", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("Goodbye world."));
}

#[test]
fn docx_tracked_edit_requires_author() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    bl().args([
        "docx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"replace","index":1,"old":"Hello","new":"Hi"}]"#,
        "--track",
        "-o",
        &path(&dir, "x.docx"),
    ])
    .assert()
    .code(2)
    .stderr(predicate::str::contains("author required"));
}

#[test]
fn docx_tracked_edit_accept_reject() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    let tracked = path(&dir, "t.docx");
    bl().env("BLACKLINE_AUTHOR", "Casey Ng")
        .args([
            "docx",
            "edit",
            &input,
            "--ops",
            r#"[{"op":"replace","index":1,"old":"Hello world.","new":"Hi world."}]"#,
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

    bl().args(["docx", "check", &tracked, "--original", &input])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"pass\""));

    let accepted = path(&dir, "accepted.docx");
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
        .stdout(predicate::str::contains("Hi world."));
}

#[test]
fn docx_redline_and_comments() {
    let dir = fixtures();
    let original = path(&dir, "redline_original.docx");
    let revised = path(&dir, "redline_revised.docx");
    let out = path(&dir, "cmp.docx");
    bl().args([
        "docx",
        "redline",
        &original,
        &revised,
        "-o",
        &out,
        "--author",
        "Riley Fox",
    ])
    .assert()
    .success();

    bl().args(["docx", "changes", &out])
        .assert()
        .success()
        .stdout(predicate::str::contains("Riley Fox"));

    bl().args(["docx", "comments", &path(&dir, "comments.docx")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Alex Rivera"));
}

#[test]
fn docx_create_parts_cat() {
    let dir = fixtures();
    let out = path(&dir, "made.docx");
    bl().args([
        "docx",
        "create",
        "--spec",
        r#"{"paragraphs":[{"text":"Fresh document"}]}"#,
        "-o",
        &out,
    ])
    .assert()
    .success();
    bl().args(["docx", "parts", &out, "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("word/document.xml"));
    bl().args(["docx", "cat", &out])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fresh document"));
}

#[test]
fn docx_dry_run_and_missing_output() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    bl().args([
        "docx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"replace","index":1,"old":"Hello world.","new":"X"}]"#,
        "--dry-run",
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("dry-run"));

    bl().args([
        "docx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"replace","index":1,"old":"Hello world.","new":"X"}]"#,
    ])
    .assert()
    .code(2)
    .stderr(predicate::str::contains("-o/--output"));
}

#[test]
fn docx_strict_bad_index_is_failure() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    bl().args([
        "docx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"replace","index":99,"old":"no","new":"x"}]"#,
        "-o",
        &path(&dir, "nope.docx"),
    ])
    .assert()
    .code(1);
}

#[test]
fn xlsx_view_edit_find() {
    let dir = fixtures();
    let input = path(&dir, "simple.xlsx");
    bl().args(["xlsx", "view", &input, "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Ada Lovelace"));
    bl().args(["xlsx", "info", &input])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sheet1"));
    bl().args(["xlsx", "find", &input, "Turing", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Alan Turing"));

    let out = path(&dir, "edited.xlsx");
    bl().args([
        "xlsx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"set_cell","sheet":"Sheet1","cell":"A1","value":"Renamed"}]"#,
        "-o",
        &out,
    ])
    .assert()
    .success();
    bl().args(["xlsx", "find", &out, "Renamed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Renamed"));
    bl().args(["xlsx", "check", &out]).assert().success();
}

#[test]
fn pptx_view_edit() {
    let dir = fixtures();
    let input = path(&dir, "simple.pptx");
    bl().args(["pptx", "view", &input])
        .assert()
        .success()
        .stdout(predicate::str::contains("Quarterly Review"));
    let out = path(&dir, "edited.pptx");
    bl().args([
        "pptx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"set_text","slide":1,"element":1,"text":"Annual Review"}]"#,
        "-o",
        &out,
        "--json",
    ])
    .assert()
    .success();
    bl().args(["pptx", "find", &out, "Annual"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Annual Review"));
    bl().args(["pptx", "check", &out]).assert().success();
}

#[test]
fn xml_get_edit_and_unpack_pack() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    bl().args([
        "xml",
        "get",
        &input,
        "word/document.xml",
        "--path",
        "body/p[0]",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Hello world."));

    let edited = path(&dir, "tree.docx");
    bl().args([
        "xml",
        "edit",
        &input,
        "--ops",
        r#"[{"part":"word/document.xml","action":"set_text","path":"body/p[0]/r[0]/t","text":"Tree edit"}]"#,
        "-o",
        &edited,
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &edited])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tree edit"));

    let unpacked = dir.path().join("unpacked");
    bl().args(["unpack", &input, unpacked.to_str().unwrap()])
        .assert()
        .success();
    assert!(unpacked.join("word/document.xml").exists());

    let packed = path(&dir, "repacked.docx");
    bl().args(["pack", unpacked.to_str().unwrap(), &packed])
        .assert()
        .success();
    bl().args(["docx", "view", &packed])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello world."));
}

#[test]
fn invalid_ops_json_is_usage_error() {
    let dir = fixtures();
    bl().args([
        "docx",
        "edit",
        &path(&dir, "simple.docx"),
        "--ops",
        "not-json",
        "-o",
        &path(&dir, "x.docx"),
    ])
    .assert()
    .code(2)
    .stderr(predicate::str::contains("invalid ops JSON"));
}

#[test]
fn both_binaries_exist() {
    Command::cargo_bin("blackline")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("docx"));
    Command::cargo_bin("blackline")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("blackline"));
}

#[test]
fn xlsx_and_pptx_create() {
    let dir = fixtures();
    let xlsx = path(&dir, "made.xlsx");
    bl().args([
        "xlsx",
        "create",
        "--spec",
        r#"{"sheets":[{"name":"S","rows":[["hello","world"]]}]}"#,
        "-o",
        &xlsx,
    ])
    .assert()
    .success();
    bl().args(["xlsx", "find", &xlsx, "hello"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));
    bl().args(["xlsx", "check", &xlsx]).assert().success();

    let pptx = path(&dir, "made.pptx");
    bl().args([
        "pptx",
        "create",
        "--spec",
        r#"{"slides":[{"texts":["Deck title"]}]}"#,
        "-o",
        &pptx,
    ])
    .assert()
    .success();
    bl().args(["pptx", "view", &pptx])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deck title"));
    bl().args(["pptx", "check", &pptx]).assert().success();
}

#[test]
fn in_place_edit_and_output_conflict() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    bl().args([
        "docx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"replace","index":1,"old":"Hello world.","new":"In place."}]"#,
        "--in-place",
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &input])
        .assert()
        .success()
        .stdout(predicate::str::contains("In place."));

    bl().args([
        "docx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"replace","index":1,"old":"In place.","new":"X"}]"#,
        "--in-place",
        "-o",
        &path(&dir, "also.docx"),
    ])
    .assert()
    .code(2)
    .stderr(predicate::str::contains("in-place"));
}

#[test]
fn lenient_applies_what_it_can() {
    let dir = fixtures();
    let input = path(&dir, "simple.docx");
    let output = path(&dir, "lenient.docx");
    bl().args([
        "docx",
        "edit",
        &input,
        "--ops",
        r#"[{"op":"replace","index":1,"old":"Hello world.","new":"Patched."},{"op":"replace","index":99,"old":"no","new":"x"}]"#,
        "-o",
        &output,
        "--lenient",
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"applied\": 1"))
    .stdout(predicate::str::contains("\"failed\": 1"));
    bl().args(["docx", "view", &output])
        .assert()
        .success()
        .stdout(predicate::str::contains("Patched."));
}

#[test]
fn xml_eval_and_select_formulas() {
    let dir = fixtures();
    let simple = path(&dir, "simple.docx");
    let tracked = path(&dir, "tracked.docx");

    bl().args([
        "xml",
        "eval",
        &simple,
        "word/document.xml",
        r#"contains(text(//t), "Hello")"#,
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"kind\": \"bool\""))
    .stdout(predicate::str::contains("\"value\": true"));

    bl().args([
        "xml",
        "eval",
        &tracked,
        "word/document.xml",
        "exists(//ins)",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("true"));

    bl().args(["xml", "select", &tracked, "word/document.xml", "//del"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"count\""))
        .stdout(predicate::str::contains("w:del"));

    bl().args(["xml", "eval", &simple, "word/document.xml", "count("])
        .assert()
        .code(2);

    bl().args(["xml", "select", &simple, "word/document.xml", "count(//p)"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("xml eval"));
}

#[test]
fn xml_patch_and_update_compile_to_treeops() {
    let dir = fixtures();
    let tracked = path(&dir, "tracked.docx");
    let simple = path(&dir, "simple.docx");

    bl().args([
        "xml",
        "update",
        &tracked,
        "--part",
        "word/document.xml",
        "--ops",
        "delete nodes //ins",
        "--dry-run",
        "--json",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"dry_run\": true"))
    .stdout(predicate::str::contains("remove_child"));

    let cleaned = path(&dir, "no-ins.docx");
    bl().args([
        "xml",
        "update",
        &tracked,
        "--part",
        "word/document.xml",
        "--ops",
        "delete nodes //ins",
        "-o",
        &cleaned,
    ])
    .assert()
    .success();
    bl().args([
        "xml",
        "eval",
        &cleaned,
        "word/document.xml",
        "exists(//ins)",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("false"));

    let patched = path(&dir, "patched.docx");
    bl().args([
        "xml",
        "patch",
        &simple,
        "--part",
        "word/document.xml",
        "--ops",
        r#"[{"op":"replace","sel":"//t[0]","text":"Patched via RFC 5261"}]"#,
        "-o",
        &patched,
    ])
    .assert()
    .success();
    bl().args(["docx", "view", &patched])
        .assert()
        .success()
        .stdout(predicate::str::contains("Patched via RFC 5261"));
}

#[test]
fn fixtures_pass_package_check() {
    let dir = fixtures();
    for name in [
        "simple.docx",
        "empty.docx",
        "headings.docx",
        "formatted.docx",
        "mixed_runs.docx",
        "special_chars.docx",
        "table.docx",
        "long.docx",
        "tracked.docx",
        "comments.docx",
        "redline.docx",
        "simple.xlsx",
        "empty.xlsx",
        "formulas.xlsx",
        "multi_sheet.xlsx",
        "large.xlsx",
        "simple.pptx",
        "multi_slide.pptx",
        "long_deck.pptx",
    ] {
        let file = path(&dir, name);
        let format = if name.ends_with(".xlsx") {
            "xlsx"
        } else if name.ends_with(".pptx") {
            "pptx"
        } else {
            "docx"
        };
        bl().args([format, "check", &file])
            .assert()
            .success()
            .stdout(predicate::str::contains("\"status\": \"pass\""));
    }
}

#[test]
fn ai_help_is_on_the_main_cli() {
    bl().args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Local AI"));
    bl().args(["ai", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("phi-3.5"))
        .stdout(predicate::str::contains("--model"))
        .stdout(predicate::str::contains("--author"))
        .stdout(predicate::str::contains("--dry-run"));
}

#[test]
fn ai_refuses_pdf_and_requires_author() {
    let dir = fixtures();
    let pdf = {
        let p = dir.path().join("memo.pdf");
        std::fs::write(&p, b"%PDF").unwrap();
        p.display().to_string()
    };
    bl().args(["ai", &pdf, "summarize this", "--dry-run"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains(".docx"));

    let file = path(&dir, "simple.docx");
    bl().args(["ai", &file, "change hello to hi", "--dry-run"])
        .env_remove("BLACKLINE_AUTHOR")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("author required"));
}

#[test]
fn ai_without_kalosm_explains_rebuild() {
    if cfg!(feature = "kalosm") {
        return;
    }
    let dir = fixtures();
    let file = path(&dir, "simple.docx");
    bl().args([
        "ai",
        &file,
        "change hello to hi",
        "--dry-run",
        "--author",
        "Jane",
    ])
    .assert()
    .failure()
    .code(2)
    .stderr(predicate::str::contains("--features kalosm"));
}
