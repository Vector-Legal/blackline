//! Sheet views and search.

use serde::Serialize;

use blackline_core::package::Package;
use blackline_core::textutil::find_normalized_ci;

use crate::error::XlsxError;
use crate::workbook::{
    decode_cell, list_sheets, shared_strings, sheet_by_name_or_index, sheet_cells, CellValue,
};

/// One sheet's rendered rows.
#[derive(Debug, Clone, Serialize)]
pub struct SheetView {
    /// Sheet name.
    pub sheet: String,
    /// `A1\tvalue` lines.
    pub rows: Vec<String>,
}

/// A search hit.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    /// Sheet.
    pub sheet: String,
    /// Cell ref.
    pub cell: String,
    /// Cell text.
    pub text: String,
}

/// View one or all sheets.
pub fn view(pkg: &Package, sheet: Option<&str>) -> Result<Vec<SheetView>, XlsxError> {
    let sheets = list_sheets(pkg)?;
    let strings = shared_strings(pkg)?;
    let selected: Vec<_> = if let Some(key) = sheet {
        vec![sheet_by_name_or_index(&sheets, key)?.clone()]
    } else {
        sheets
    };
    let mut out = Vec::new();
    for info in selected {
        let doc = pkg.part_xml(&info.part)?;
        let mut rows = Vec::new();
        for (r, c) in sheet_cells(&doc) {
            let val = decode_cell(c, &strings);
            let text = display(&val);
            if !text.is_empty() {
                rows.push(format!("{r}\t{text}"));
            }
        }
        out.push(SheetView {
            sheet: info.name,
            rows,
        });
    }
    Ok(out)
}

/// Search cell text.
pub fn find(pkg: &Package, query: &str, limit: usize) -> Result<Vec<SearchHit>, XlsxError> {
    let sheets = list_sheets(pkg)?;
    let strings = shared_strings(pkg)?;
    let mut hits = Vec::new();
    for info in sheets {
        let doc = pkg.part_xml(&info.part)?;
        for (cell, c) in sheet_cells(&doc) {
            let text = display(&decode_cell(c, &strings));
            if find_normalized_ci(&text, query).is_some() {
                hits.push(SearchHit {
                    sheet: info.name.clone(),
                    cell,
                    text,
                });
                if limit > 0 && hits.len() >= limit {
                    return Ok(hits);
                }
            }
        }
    }
    Ok(hits)
}

fn display(v: &CellValue) -> String {
    match v {
        CellValue::Empty => String::new(),
        CellValue::Number(n) => n.to_string(),
        CellValue::Text(s) => s.clone(),
        CellValue::Formula { formula, cached } => {
            cached.clone().unwrap_or_else(|| format!("={formula}"))
        }
        CellValue::Bool(b) => b.to_string(),
    }
}
