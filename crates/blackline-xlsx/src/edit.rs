//! Workbook edit operations.

use serde::{Deserialize, Serialize};

use blackline_core::package::Package;
use blackline_core::xml::XmlNode;

use crate::cell::{format_cell_ref, parse_cell_ref, parse_range};
use crate::error::XlsxError;
use crate::workbook::{
    encode_cell, list_sheets, set_shared_strings, shared_strings, sheet_by_name_or_index, CellValue,
};

/// Edit options.
#[derive(Debug, Clone, Default)]
pub struct EditOptions {
    /// Best-effort mode.
    pub lenient: bool,
    /// Don't write.
    pub dry_run: bool,
}

/// One edit op.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op")]
pub enum EditOp {
    /// Set a single cell.
    #[serde(rename = "set_cell")]
    SetCell {
        /// Sheet name or 1-based index.
        sheet: String,
        /// A1 reference.
        cell: String,
        /// Value. String, number, bool, or `=FORMULA`.
        #[serde(default)]
        value: Option<serde_json::Value>,
        /// Explicit formula (overrides `value` if both given).
        #[serde(default)]
        formula: Option<String>,
    },
    /// Set a rectangular range from a row-major grid.
    #[serde(rename = "set_range")]
    SetRange {
        /// Sheet.
        sheet: String,
        /// `A1:C3`.
        range: String,
        /// Row-major values.
        values: Vec<Vec<serde_json::Value>>,
    },
    /// Insert an empty row at 1-based `row`.
    #[serde(rename = "insert_row")]
    InsertRow {
        /// Sheet.
        sheet: String,
        /// 1-based row.
        row: usize,
    },
    /// Delete a row.
    #[serde(rename = "delete_row")]
    DeleteRow {
        /// Sheet.
        sheet: String,
        /// 1-based row.
        row: usize,
    },
    /// Rename a sheet.
    #[serde(rename = "set_sheet_name")]
    SetSheetName {
        /// Current name or index.
        sheet: String,
        /// New name.
        name: String,
    },
    /// Add an empty sheet.
    #[serde(rename = "insert_sheet")]
    InsertSheet {
        /// New name.
        name: String,
    },
    /// Delete a sheet.
    #[serde(rename = "delete_sheet")]
    DeleteSheet {
        /// Sheet.
        sheet: String,
    },
}

impl EditOp {
    /// Op name.
    pub fn name(&self) -> &'static str {
        match self {
            EditOp::SetCell { .. } => "set_cell",
            EditOp::SetRange { .. } => "set_range",
            EditOp::InsertRow { .. } => "insert_row",
            EditOp::DeleteRow { .. } => "delete_row",
            EditOp::SetSheetName { .. } => "set_sheet_name",
            EditOp::InsertSheet { .. } => "insert_sheet",
            EditOp::DeleteSheet { .. } => "delete_sheet",
        }
    }
}

/// Batch report.
#[derive(Debug, Clone, Serialize)]
pub struct EditReport {
    /// Applied.
    pub applied: usize,
    /// Failed.
    pub failed: usize,
    /// Mode.
    pub mode: &'static str,
    /// Per-op rows.
    pub ops: Vec<OpReport>,
}

pub(crate) mod edit_report {
    use serde::Serialize;
    /// Per-op status.
    #[derive(Debug, Clone, Serialize)]
    pub struct OpReport {
        /// Index.
        pub index: usize,
        /// Name.
        pub op: String,
        /// Status.
        pub status: &'static str,
        /// Detail.
        pub detail: String,
    }
}

use edit_report::OpReport;

/// Apply ops.
pub fn apply(
    pkg: &mut Package,
    ops: &[EditOp],
    opts: &EditOptions,
) -> Result<EditReport, XlsxError> {
    let mut reports = Vec::new();
    let mut strings = shared_strings(pkg)?;
    for (i, op) in ops.iter().enumerate() {
        if opts.dry_run {
            reports.push(OpReport {
                index: i,
                op: op.name().into(),
                status: "applied",
                detail: "dry-run".into(),
            });
            continue;
        }
        match apply_one(pkg, op, &mut strings) {
            Ok(d) => reports.push(OpReport {
                index: i,
                op: op.name().into(),
                status: "applied",
                detail: d,
            }),
            Err(e) => {
                reports.push(OpReport {
                    index: i,
                    op: op.name().into(),
                    status: "failed",
                    detail: e.to_string(),
                });
                if !opts.lenient {
                    return Err(XlsxError::OpFailed {
                        index: i,
                        op: op.name().into(),
                        reason: e.to_string(),
                    });
                }
            }
        }
    }
    if !opts.dry_run {
        set_shared_strings(pkg, &strings)?;
    }
    let applied = reports.iter().filter(|r| r.status == "applied").count();
    let failed = reports.iter().filter(|r| r.status == "failed").count();
    Ok(EditReport {
        applied,
        failed,
        mode: if opts.dry_run {
            "dry-run"
        } else if opts.lenient {
            "lenient"
        } else {
            "strict"
        },
        ops: reports,
    })
}

fn apply_one(
    pkg: &mut Package,
    op: &EditOp,
    strings: &mut Vec<String>,
) -> Result<String, XlsxError> {
    match op {
        EditOp::SetCell {
            sheet,
            cell,
            value,
            formula,
        } => {
            let val = if let Some(f) = formula {
                CellValue::Formula {
                    formula: f.trim_start_matches('=').to_string(),
                    cached: None,
                }
            } else {
                json_to_cell(value.as_ref().unwrap_or(&serde_json::Value::Null))
            };
            set_one_cell(pkg, sheet, cell, &val, strings)?;
            Ok(format!("set {sheet}!{cell}"))
        }
        EditOp::SetRange {
            sheet,
            range,
            values,
        } => {
            let ((r0, c0), _) = parse_range(range)?;
            for (i, row) in values.iter().enumerate() {
                for (j, v) in row.iter().enumerate() {
                    let cell = format_cell_ref(r0 + i, c0 + j);
                    set_one_cell(pkg, sheet, &cell, &json_to_cell(v), strings)?;
                }
            }
            Ok(format!("set range {sheet}!{range}"))
        }
        EditOp::InsertRow { sheet, row } => {
            shift_rows(pkg, sheet, *row, 1)?;
            Ok(format!("inserted row {row}"))
        }
        EditOp::DeleteRow { sheet, row } => {
            delete_row(pkg, sheet, *row)?;
            Ok(format!("deleted row {row}"))
        }
        EditOp::SetSheetName { sheet, name } => {
            rename_sheet(pkg, sheet, name)?;
            Ok(format!("renamed to {name}"))
        }
        EditOp::InsertSheet { name } => {
            append_empty_sheet(pkg, name)?;
            Ok(format!("inserted sheet {name}"))
        }
        EditOp::DeleteSheet { sheet } => {
            delete_sheet(pkg, sheet)?;
            Ok(format!("deleted sheet {sheet}"))
        }
    }
}

fn json_to_cell(v: &serde_json::Value) -> CellValue {
    match v {
        serde_json::Value::Null => CellValue::Empty,
        serde_json::Value::Bool(b) => CellValue::Bool(*b),
        serde_json::Value::Number(n) => CellValue::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) if s.starts_with('=') => CellValue::Formula {
            formula: s[1..].to_string(),
            cached: None,
        },
        serde_json::Value::String(s) => CellValue::Text(s.clone()),
        other => CellValue::Text(other.to_string()),
    }
}

fn set_one_cell(
    pkg: &mut Package,
    sheet: &str,
    cell: &str,
    value: &CellValue,
    strings: &mut Vec<String>,
) -> Result<(), XlsxError> {
    let (row_n, _) = parse_cell_ref(cell)?;
    let sheets = list_sheets(pkg)?;
    let info = sheet_by_name_or_index(&sheets, sheet)?.clone();
    let mut doc = pkg.part_xml(&info.part)?;
    let sheet_data = doc
        .root
        .find_child_mut("sheetData")
        .ok_or_else(|| XlsxError::invalid("no sheetData"))?;

    // Find or create the row.
    let mut row_i = None;
    for (i, child) in sheet_data.children().iter().enumerate() {
        if child.is_element_with_local_name("row")
            && child.get_attr("r") == Some(&row_n.to_string())
        {
            row_i = Some(i);
            break;
        }
    }
    if row_i.is_none() {
        sheet_data
            .children_mut()
            .push(XmlNode::element("row").with_attr("r", row_n.to_string()));
        row_i = Some(sheet_data.children().len() - 1);
    }
    let row = &mut sheet_data.children_mut()[row_i.unwrap()];
    let encoded = encode_cell(cell, value, strings);
    if let Some(existing) = row
        .children_mut()
        .iter_mut()
        .find(|c| c.is_element_with_local_name("c") && c.get_attr("r") == Some(cell))
    {
        *existing = encoded;
    } else {
        row.children_mut().push(encoded);
    }
    pkg.set_part_xml(&info.part, &doc);
    Ok(())
}

fn shift_rows(pkg: &mut Package, sheet: &str, at: usize, delta: isize) -> Result<(), XlsxError> {
    let sheets = list_sheets(pkg)?;
    let info = sheet_by_name_or_index(&sheets, sheet)?.clone();
    let mut doc = pkg.part_xml(&info.part)?;
    let Some(sd) = doc.root.find_child_mut("sheetData") else {
        return Ok(());
    };
    for row in sd.children_mut() {
        if !row.is_element_with_local_name("row") {
            continue;
        }
        let r: usize = row.get_attr("r").and_then(|s| s.parse().ok()).unwrap_or(0);
        if r >= at {
            let new_r = (r as isize + delta) as usize;
            row.set_attr("r", &new_r.to_string());
            for c in row.children_mut() {
                if let Some(pref) = c.get_attr("r") {
                    if let Ok((cr, cc)) = parse_cell_ref(pref) {
                        if cr >= at {
                            c.set_attr("r", &format_cell_ref((cr as isize + delta) as usize, cc));
                        }
                    }
                }
            }
        }
    }
    pkg.set_part_xml(&info.part, &doc);
    Ok(())
}

fn delete_row(pkg: &mut Package, sheet: &str, row: usize) -> Result<(), XlsxError> {
    let sheets = list_sheets(pkg)?;
    let info = sheet_by_name_or_index(&sheets, sheet)?.clone();
    let mut doc = pkg.part_xml(&info.part)?;
    if let Some(sd) = doc.root.find_child_mut("sheetData") {
        sd.children_mut().retain(|c| {
            !(c.is_element_with_local_name("row") && c.get_attr("r") == Some(&row.to_string()))
        });
    }
    pkg.set_part_xml(&info.part, &doc);
    shift_rows(pkg, sheet, row + 1, -1)?;
    Ok(())
}

fn rename_sheet(pkg: &mut Package, sheet: &str, name: &str) -> Result<(), XlsxError> {
    let sheets = list_sheets(pkg)?;
    let info = sheet_by_name_or_index(&sheets, sheet)?.clone();
    let mut wb = pkg.part_xml("xl/workbook.xml")?;
    for n in wb.root.find_all("sheet") {
        // need mut — walk children
        let _ = n;
    }
    if let Some(sheets_el) = wb.root.find_child_mut("sheets") {
        for child in sheets_el.children_mut() {
            if child.get_attr("name") == Some(&info.name) {
                child.set_attr("name", name);
            }
        }
    }
    pkg.set_part_xml("xl/workbook.xml", &wb);
    Ok(())
}

fn append_empty_sheet(pkg: &mut Package, name: &str) -> Result<(), XlsxError> {
    let existing = list_sheets(pkg)?;
    let n = existing.len() + 1;
    let part = format!("xl/worksheets/sheet{n}.xml");
    let xml = blackline_core::XmlDocument::new(
        XmlNode::element("worksheet")
            .with_attr("xmlns", blackline_core::ns::S)
            .with_child(XmlNode::element("sheetData")),
    );
    pkg.set_part_xml(&part, &xml);
    let mut ct = pkg.content_types()?;
    ct.ensure_override(&part, blackline_core::ns::content::WORKSHEET);
    pkg.set_content_types(&ct);

    let mut rels = pkg.rels_for("xl/workbook.xml")?;
    let rid = rels.add(blackline_core::Relationship::internal(
        String::new(),
        blackline_core::ns::rel::WORKSHEET,
        format!("worksheets/sheet{n}.xml"),
    ));
    pkg.set_rels_for("xl/workbook.xml", &rels);

    let mut wb = pkg.part_xml("xl/workbook.xml")?;
    if let Some(sheets) = wb.root.find_child_mut("sheets") {
        sheets.children_mut().push(
            XmlNode::element("sheet")
                .with_attr("name", name)
                .with_attr("sheetId", n.to_string())
                .with_attr("r:id", &rid),
        );
    }
    pkg.set_part_xml("xl/workbook.xml", &wb);
    Ok(())
}

fn delete_sheet(pkg: &mut Package, sheet: &str) -> Result<(), XlsxError> {
    let sheets = list_sheets(pkg)?;
    if sheets.len() <= 1 {
        return Err(XlsxError::invalid("cannot delete the last sheet"));
    }
    let info = sheet_by_name_or_index(&sheets, sheet)?.clone();
    pkg.remove_part(&info.part);

    let mut rels = pkg.rels_for("xl/workbook.xml")?;
    let rid = rels
        .items
        .iter()
        .find(|r| blackline_core::rels::resolve_target("xl/workbook.xml", &r.target) == info.part)
        .map(|r| r.id.clone());
    if let Some(id) = rid {
        rels.remove(&id);
        pkg.set_rels_for("xl/workbook.xml", &rels);
    }

    let mut ct = pkg.content_types()?;
    ct.remove_override(&info.part);
    pkg.set_content_types(&ct);

    let mut wb = pkg.part_xml("xl/workbook.xml")?;
    if let Some(el) = wb.root.find_child_mut("sheets") {
        el.children_mut()
            .retain(|c| c.get_attr("name") != Some(&info.name));
    }
    pkg.set_part_xml("xl/workbook.xml", &wb);
    Ok(())
}
