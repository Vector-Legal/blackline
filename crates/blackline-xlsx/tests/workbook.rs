//! XLSX create / edit / reopen precision tests.

use blackline_xlsx::{CellValue, EditOp, EditOptions, Xlsx};

fn apply(wb: &mut Xlsx, ops: Vec<EditOp>) {
    wb.edit(&ops, &EditOptions::default()).unwrap();
}

#[test]
fn from_rows_roundtrip() {
    let wb = Xlsx::from_rows(
        "People",
        &[
            vec!["Name", "City"],
            vec!["Ada", "London"],
            vec!["Alan", "Manchester"],
        ],
    )
    .unwrap();
    assert_eq!(wb.sheets().unwrap()[0].name, "People");
    match wb.cell("People", "A2").unwrap() {
        CellValue::Text(s) => assert_eq!(s, "Ada"),
        other => panic!("expected text, got {other:?}"),
    }
    assert!(wb.check().unwrap().passed());
}

#[test]
fn save_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.xlsx");
    let wb = Xlsx::from_rows("S", &[vec!["hello", "world"]]).unwrap();
    wb.save(&path).unwrap();
    let again = Xlsx::open(&path).unwrap();
    match again.cell("S", "B1").unwrap() {
        CellValue::Text(s) => assert_eq!(s, "world"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn set_cell_number_text_bool_formula() {
    let mut wb = Xlsx::from_rows("Sheet1", &[vec!["x"]]).unwrap();
    apply(
        &mut wb,
        vec![
            EditOp::SetCell {
                sheet: "Sheet1".into(),
                cell: "A1".into(),
                value: Some(serde_json::json!(42)),
                formula: None,
            },
            EditOp::SetCell {
                sheet: "Sheet1".into(),
                cell: "B1".into(),
                value: Some(serde_json::json!("hello")),
                formula: None,
            },
            EditOp::SetCell {
                sheet: "Sheet1".into(),
                cell: "C1".into(),
                value: Some(serde_json::json!(true)),
                formula: None,
            },
            EditOp::SetCell {
                sheet: "Sheet1".into(),
                cell: "D1".into(),
                value: None,
                formula: Some("SUM(A1:A1)".into()),
            },
        ],
    );
    assert!(matches!(wb.cell("Sheet1", "A1").unwrap(), CellValue::Number(n) if n == 42.0));
    assert!(matches!(wb.cell("Sheet1", "B1").unwrap(), CellValue::Text(s) if s == "hello"));
    assert!(matches!(
        wb.cell("Sheet1", "C1").unwrap(),
        CellValue::Bool(true)
    ));
    assert!(matches!(
        wb.cell("Sheet1", "D1").unwrap(),
        CellValue::Formula { formula, .. } if formula == "SUM(A1:A1)"
    ));
    assert!(wb.check().unwrap().passed());
}

#[test]
fn set_range_and_find() {
    let mut wb = Xlsx::from_rows("Sheet1", &[vec!["old"]]).unwrap();
    apply(
        &mut wb,
        vec![EditOp::SetRange {
            sheet: "1".into(),
            range: "A1:B2".into(),
            values: vec![
                vec![serde_json::json!("alpha"), serde_json::json!("beta")],
                vec![serde_json::json!("gamma"), serde_json::json!("delta")],
            ],
        }],
    );
    let hits = wb.find("gamma", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].cell, "A2");
}

#[test]
fn insert_and_delete_row_shifts_refs() {
    let mut wb = Xlsx::from_rows("Sheet1", &[vec!["r1"], vec!["r2"], vec!["r3"]]).unwrap();
    apply(
        &mut wb,
        vec![EditOp::InsertRow {
            sheet: "Sheet1".into(),
            row: 2,
        }],
    );
    assert!(matches!(wb.cell("Sheet1", "A3").unwrap(), CellValue::Text(s) if s == "r2"));
    apply(
        &mut wb,
        vec![EditOp::DeleteRow {
            sheet: "Sheet1".into(),
            row: 2,
        }],
    );
    assert!(matches!(wb.cell("Sheet1", "A2").unwrap(), CellValue::Text(s) if s == "r2"));
}

#[test]
fn insert_rename_delete_sheet() {
    let mut wb = Xlsx::from_rows("Sheet1", &[vec!["keep"]]).unwrap();
    apply(
        &mut wb,
        vec![
            EditOp::InsertSheet {
                name: "Extra".into(),
            },
            EditOp::SetSheetName {
                sheet: "Extra".into(),
                name: "Renamed".into(),
            },
        ],
    );
    let names: Vec<String> = wb.sheets().unwrap().into_iter().map(|s| s.name).collect();
    assert!(names.contains(&"Sheet1".into()));
    assert!(names.contains(&"Renamed".into()));
    apply(
        &mut wb,
        vec![EditOp::DeleteSheet {
            sheet: "Renamed".into(),
        }],
    );
    assert_eq!(wb.sheets().unwrap().len(), 1);
    assert!(wb.check().unwrap().passed());
}

#[test]
fn set_sheet_name_by_index() {
    let mut wb = Xlsx::from_rows("Sheet1", &[vec!["keep"]]).unwrap();
    apply(
        &mut wb,
        vec![EditOp::SetSheetName {
            sheet: "1".into(),
            name: "Indexed".into(),
        }],
    );
    assert_eq!(wb.sheets().unwrap()[0].name, "Indexed");
}

#[test]
fn view_and_info_expose_cells() {
    let wb = Xlsx::from_rows("People", &[vec!["Name", "City"], vec!["Ada", "London"]]).unwrap();
    let info = wb.info().unwrap();
    assert_eq!(info.sheets, 1);
    assert_eq!(info.sheet_names, ["People"]);
    let view = wb.view(Some("People")).unwrap();
    assert_eq!(view.len(), 1);
    assert!(view[0].rows.iter().any(|r| r.contains("Ada")));
    let by_index = wb.view(Some("1")).unwrap();
    assert_eq!(by_index[0].sheet, "People");
}

#[test]
fn empty_cell_clears_value() {
    let mut wb = Xlsx::from_rows("Sheet1", &[vec!["hello"]]).unwrap();
    apply(
        &mut wb,
        vec![EditOp::SetCell {
            sheet: "Sheet1".into(),
            cell: "A1".into(),
            value: None,
            formula: None,
        }],
    );
    assert!(matches!(wb.cell("Sheet1", "A1").unwrap(), CellValue::Empty));
}

#[test]
fn dry_run_does_not_mutate() {
    let mut wb = Xlsx::from_rows("Sheet1", &[vec!["hello"]]).unwrap();
    let report = wb
        .edit(
            &[EditOp::SetCell {
                sheet: "Sheet1".into(),
                cell: "A1".into(),
                value: Some(serde_json::json!("ghost")),
                formula: None,
            }],
            &EditOptions {
                dry_run: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(report.mode, "dry-run");
    assert!(matches!(wb.cell("Sheet1", "A1").unwrap(), CellValue::Text(s) if s == "hello"));
}

#[test]
fn cannot_delete_last_sheet() {
    let mut wb = Xlsx::from_rows("Only", &[vec!["x"]]).unwrap();
    let err = wb.edit(
        &[EditOp::DeleteSheet {
            sheet: "Only".into(),
        }],
        &EditOptions::default(),
    );
    assert!(err.is_err());
}

#[test]
fn shared_strings_are_reused() {
    let mut wb = Xlsx::from_rows("S", &[vec!["same"]]).unwrap();
    apply(
        &mut wb,
        vec![
            EditOp::SetCell {
                sheet: "S".into(),
                cell: "A2".into(),
                value: Some(serde_json::json!("same")),
                formula: None,
            },
            EditOp::SetCell {
                sheet: "S".into(),
                cell: "A3".into(),
                value: Some(serde_json::json!("same")),
                formula: None,
            },
        ],
    );
    let xml = String::from_utf8_lossy(wb.package().part("xl/sharedStrings.xml").unwrap());
    // uniqueCount should be 1
    assert!(xml.contains("uniqueCount=\"1\"") || xml.matches(">same<").count() == 1);
}
