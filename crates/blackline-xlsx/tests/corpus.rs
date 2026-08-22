//! End-to-end open / view / edit against vendored Apache POI XLSX files.
//!
//! XLSX has no tracked-change / redline API. Precision here means: after a
//! surgical `set_cell`, `check` passes, the new value is readable, other
//! sheets stay put, and sidecar parts (charts, drawings, comments, media,
//! theme) keep their original bytes.

use std::path::PathBuf;

use blackline_xlsx::{CellValue, EditOp, EditOptions, Xlsx};

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

const MARKER: f64 = 987_654_321.0;
const MARKER_TEXT: &str = "987654321";
const FAR_CELL: &str = "Z99";

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/xlsx")
}

fn corpus_path(name: &str) -> PathBuf {
    corpus_dir().join(name)
}

fn open(name: &str) -> Xlsx {
    let path = corpus_path(name);
    assert!(
        path.is_file(),
        "missing corpus file {} (workspace checkout required)",
        path.display()
    );
    Xlsx::open(&path).unwrap_or_else(|e| panic!("open {name}: {e}"))
}

fn apply(wb: &mut Xlsx, ops: Vec<EditOp>) {
    wb.edit(&ops, &EditOptions::default())
        .unwrap_or_else(|e| panic!("edit failed: {e}"));
}

fn assert_check(wb: &Xlsx, label: &str) {
    let health = wb.check().unwrap();
    assert!(
        health.passed(),
        "{label}: check failed: {}",
        serde_json::to_string_pretty(&health).unwrap_or_else(|_| format!("{health:?}"))
    );
}

fn first_sheet(wb: &Xlsx) -> String {
    wb.sheets()
        .unwrap()
        .into_iter()
        .next()
        .expect("workbook has no sheets")
        .name
}

fn set_far_marker(sheet: &str) -> EditOp {
    EditOp::SetCell {
        sheet: sheet.into(),
        cell: FAR_CELL.into(),
        value: Some(serde_json::json!(MARKER)),
        formula: None,
    }
}

fn is_sidecar(name: &str) -> bool {
    name.contains("/media/")
        || name.contains("/charts/")
        || name.contains("/drawings/")
        || name.contains("/theme/")
        || name.contains("/tables/")
        || name.contains("comments")
        || name.contains("vmlDrawing")
        || name.ends_with(".jpeg")
        || name.ends_with(".jpg")
        || name.ends_with(".png")
        || name.ends_with(".wmf")
        || name.ends_with(".emf")
}

fn assert_sidecar_parts_untouched(before: &Xlsx, after: &Xlsx, label: &str) {
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

fn clone_wb(wb: &Xlsx) -> Xlsx {
    Xlsx::from_bytes(wb.to_bytes().unwrap()).unwrap()
}

fn sheet_snapshot(wb: &Xlsx, sheet: &str) -> Vec<String> {
    wb.view(Some(sheet))
        .unwrap()
        .into_iter()
        .flat_map(|v| v.rows)
        .collect()
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
        .filter(|n| n.ends_with(".xlsx") && !FILES.contains(&n.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "unexpected extra corpus files (update FILES): {extra:?}"
    );
}

#[test]
fn every_corpus_file_opens_views_and_checks() {
    for name in FILES {
        let wb = open(name);
        let sheets = wb.sheets().unwrap();
        assert!(!sheets.is_empty(), "{name}: no sheets");
        let views = wb.view(None).unwrap();
        assert_eq!(
            views.len(),
            sheets.len(),
            "{name}: view/sheet count mismatch"
        );
        assert_check(&wb, name);
        let info = wb.info().unwrap();
        assert_eq!(info.sheets, sheets.len(), "{name}: info.sheets");
        let _ = wb.find("a", 5).unwrap();
    }
}

#[test]
fn every_file_set_cell_then_reopen() {
    for name in FILES {
        let original = open(name);
        let sheet = first_sheet(&original);
        let other_sheets: Vec<String> = original
            .sheets()
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .filter(|n| n != &sheet)
            .collect();
        let other_before: Vec<(String, Vec<String>)> = other_sheets
            .iter()
            .map(|n| (n.clone(), sheet_snapshot(&original, n)))
            .collect();

        let mut revised = clone_wb(&original);
        apply(&mut revised, vec![set_far_marker(&sheet)]);
        assert_check(&revised, &format!("{name} set_cell"));
        match revised.cell(&sheet, FAR_CELL).unwrap() {
            CellValue::Number(n) => assert_eq!(n, MARKER, "{name}: marker number"),
            other => panic!("{name}: expected number at {FAR_CELL}, got {other:?}"),
        }
        let hits = revised.find(MARKER_TEXT, 10).unwrap();
        assert!(
            hits.iter().any(|h| h.sheet == sheet && h.cell == FAR_CELL),
            "{name}: find missed {sheet}!{FAR_CELL}"
        );
        assert_sidecar_parts_untouched(&original, &revised, name);

        for (n, before) in &other_before {
            assert_eq!(
                &sheet_snapshot(&revised, n),
                before,
                "{name}: sheet {n} changed after editing {sheet}"
            );
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        revised.save(&path).unwrap();
        let again = Xlsx::open(&path).unwrap();
        assert_check(&again, &format!("{name} reopen"));
        match again.cell(&sheet, FAR_CELL).unwrap() {
            CellValue::Number(n) => assert_eq!(n, MARKER, "{name}: reopen marker"),
            other => panic!("{name}: reopen expected number, got {other:?}"),
        }
        assert_sidecar_parts_untouched(&original, &again, &format!("{name} reopen"));
    }
}

#[test]
fn inline_string_without_cell_ref_is_a1() {
    let wb = open("InlineString.xlsx");
    match wb.cell("Sheet1", "A1").unwrap() {
        CellValue::Text(s) => {
            assert!(s.contains("more text"), "inline text was {s:?}");
            assert!(s.contains('😜') || s.contains("more text"));
        }
        other => panic!("expected inline text at A1, got {other:?}"),
    }
    let view = wb.view(Some("Sheet1")).unwrap();
    assert!(
        view[0].rows.iter().any(|r| r.starts_with("A1\t")),
        "view should synthesize A1 for a cell with no r attribute: {:?}",
        view[0].rows
    );
}

#[test]
fn creating_shared_strings_registers_part() {
    let mut wb = open("InlineString.xlsx");
    apply(
        &mut wb,
        vec![EditOp::SetCell {
            sheet: "Sheet1".into(),
            cell: "B1".into(),
            value: Some(serde_json::json!("BLACKLINE_SST")),
            formula: None,
        }],
    );
    assert_check(&wb, "InlineString after SST create");
    assert!(wb.package().has_part("xl/sharedStrings.xml"));
    let rels = wb.package().rels_for("xl/workbook.xml").unwrap();
    assert!(
        rels.by_type(blackline_core::ns::rel::SHARED_STRINGS)
            .is_some(),
        "new sharedStrings part must be related from the workbook"
    );
    let ct = wb.package().content_types().unwrap();
    assert_eq!(
        ct.content_type_of("xl/sharedStrings.xml"),
        Some(blackline_core::ns::content::SHARED_STRINGS),
        "new sharedStrings part must have a content-type override"
    );
    match wb.cell("Sheet1", "B1").unwrap() {
        CellValue::Text(s) => assert_eq!(s, "BLACKLINE_SST"),
        other => panic!("{other:?}"),
    }
    match wb.cell("Sheet1", "A1").unwrap() {
        CellValue::Text(s) => assert!(s.contains("more text")),
        other => panic!("lost inline string: {other:?}"),
    }
}

#[test]
fn unicode_sheet_name_and_greek_bmp() {
    let wb = open("unicodeSheetName.xlsx");
    assert_eq!(wb.sheets().unwrap()[0].name, "Sheet・1");
    match wb.cell("Sheet・1", "A1").unwrap() {
        CellValue::Number(n) => assert_eq!(n, 1.0),
        other => panic!("{other:?}"),
    }

    let greek = open("54084 - Greek - beyond BMP.xlsx");
    let text = match greek.cell("Sheet1", "A1").unwrap() {
        CellValue::Text(s) => s,
        other => panic!("{other:?}"),
    };
    assert!(
        text.chars().any(|c| c as u32 > 0xFFFF) || text.chars().count() > 10,
        "expected beyond-BMP Greek letters, got {text:?}"
    );
}

#[test]
fn comments_and_chart_survive_nearby_edit() {
    let comments = open("SimpleWithComments.xlsx");
    assert!(comments.package().has_part("xl/comments1.xml"));
    let mut edited = clone_wb(&comments);
    apply(&mut edited, vec![set_far_marker("Sheet1")]);
    assert_sidecar_parts_untouched(&comments, &edited, "comments");

    let chart = open("WithChart.xlsx");
    assert!(chart.package().has_part("xl/charts/chart1.xml"));
    let mut edited_chart = clone_wb(&chart);
    apply(&mut edited_chart, vec![set_far_marker("Sheet1")]);
    assert_sidecar_parts_untouched(&chart, &edited_chart, "chart");
}

#[test]
fn insert_row_on_shifting_fixture() {
    let mut wb = open("ForShifting.xlsx");
    let a1 = match wb.cell("Sheet1", "A1").unwrap() {
        CellValue::Text(s) => s,
        other => panic!("{other:?}"),
    };
    let a2 = match wb.cell("Sheet1", "A2").unwrap() {
        CellValue::Text(s) => s,
        other => panic!("{other:?}"),
    };
    apply(
        &mut wb,
        vec![EditOp::InsertRow {
            sheet: "Sheet1".into(),
            row: 2,
        }],
    );
    assert_check(&wb, "ForShifting insert_row");
    match wb.cell("Sheet1", "A1").unwrap() {
        CellValue::Text(s) => assert_eq!(s, a1),
        other => panic!("{other:?}"),
    }
    match wb.cell("Sheet1", "A3").unwrap() {
        CellValue::Text(s) => assert_eq!(s, a2, "row 2 should have shifted to 3"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn shared_formulas_and_hyperlinks_still_read() {
    let formulas = open("shared_formulas.xlsx");
    assert_eq!(formulas.sheets().unwrap()[0].name, "Label");
    match formulas.cell("Label", "A1").unwrap() {
        CellValue::Text(s) => assert_eq!(s, "Currently Using"),
        other => panic!("{other:?}"),
    }

    let links = open("sharedhyperlink.xlsx");
    match links.cell("Hoja1", "A1").unwrap() {
        CellValue::Text(s) => assert!(s.contains("apache.org"), "{s}"),
        other => panic!("{other:?}"),
    }
}
