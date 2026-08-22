//! Build a minimal Excel-openable workbook.

use serde::Deserialize;

use blackline_core::ns;
use blackline_core::package::Package;
use blackline_core::rels::{Relationship, Relationships};
use blackline_core::time::utc_now_iso;
use blackline_core::xml::{XmlDocument, XmlNode};

use crate::cell::format_cell_ref;
use crate::error::XlsxError;
use crate::workbook::{encode_cell, CellValue};

/// Workbook specification.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSpec {
    /// Sheets.
    #[serde(default)]
    pub sheets: Vec<SheetSpec>,
}

/// One sheet.
#[derive(Debug, Clone, Deserialize)]
pub struct SheetSpec {
    /// Sheet name.
    #[serde(default = "default_sheet")]
    pub name: String,
    /// Cells as `{ "A1": "hello", "B2": 3 }` — values may be string, number, or
    /// `{ "formula": "SUM(A1:A2)" }`.
    #[serde(default)]
    pub cells: serde_json::Map<String, serde_json::Value>,
    /// Row-major grid (optional alternative to `cells`).
    #[serde(default)]
    pub rows: Vec<Vec<serde_json::Value>>,
}

fn default_sheet() -> String {
    "Sheet1".into()
}

/// Build a package.
pub fn create(spec: &CreateSpec) -> Result<Package, XlsxError> {
    let sheets = if spec.sheets.is_empty() {
        vec![SheetSpec {
            name: "Sheet1".into(),
            cells: serde_json::Map::new(),
            rows: Vec::new(),
        }]
    } else {
        spec.sheets.clone()
    };

    let mut pkg = Package::new();
    let mut ct = blackline_core::ContentTypes::office_defaults();
    ct.ensure_override("xl/workbook.xml", ns::content::WORKBOOK);
    ct.ensure_override("xl/sharedStrings.xml", ns::content::SHARED_STRINGS);
    ct.ensure_override("xl/styles.xml", ns::content::SHEET_STYLES);
    ct.ensure_override("docProps/core.xml", ns::content::CORE_PROPS);
    ct.ensure_override("docProps/app.xml", ns::content::APP_PROPS);

    let mut pkg_rels = Relationships::default();
    pkg_rels.add(Relationship::internal(
        "rId1",
        ns::rel::OFFICE_DOCUMENT,
        "xl/workbook.xml",
    ));
    pkg_rels.add(Relationship::internal(
        "rId2",
        ns::rel::CORE_PROPERTIES,
        "docProps/core.xml",
    ));
    pkg_rels.add(Relationship::internal(
        "rId3",
        ns::rel::EXTENDED_PROPERTIES,
        "docProps/app.xml",
    ));
    pkg.set_rels_for("", &pkg_rels);

    let mut wb_rels = Relationships::default();
    let mut sheet_els = Vec::new();
    let mut strings: Vec<String> = Vec::new();

    for (i, sheet) in sheets.iter().enumerate() {
        let part = format!("xl/worksheets/sheet{}.xml", i + 1);
        ct.ensure_override(&part, ns::content::WORKSHEET);
        let rid = format!("rId{}", i + 1);
        wb_rels.add(Relationship::internal(
            &rid,
            ns::rel::WORKSHEET,
            format!("worksheets/sheet{}.xml", i + 1),
        ));
        sheet_els.push(
            XmlNode::element("sheet")
                .with_attr("name", &sheet.name)
                .with_attr("sheetId", (i + 1).to_string())
                .with_attr("r:id", &rid),
        );
        let xml = sheet_xml(sheet, &mut strings)?;
        pkg.set_part_xml(&part, &xml);
    }

    let sst_rid = format!("rId{}", sheets.len() + 1);
    let styles_rid = format!("rId{}", sheets.len() + 2);
    wb_rels.add(Relationship::internal(
        &sst_rid,
        ns::rel::SHARED_STRINGS,
        "sharedStrings.xml",
    ));
    wb_rels.add(Relationship::internal(
        &styles_rid,
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
        "styles.xml",
    ));
    pkg.set_rels_for("xl/workbook.xml", &wb_rels);

    let mut sheets_node = XmlNode::element("sheets");
    for el in sheet_els {
        sheets_node = sheets_node.with_child(el);
    }
    let wb = XmlDocument::new(
        XmlNode::element("workbook")
            .with_attr("xmlns", ns::S)
            .with_attr("xmlns:r", ns::R)
            .with_child(sheets_node),
    );
    pkg.set_part_xml("xl/workbook.xml", &wb);
    crate::workbook::set_shared_strings(&mut pkg, &strings)?;
    pkg.set_part("xl/styles.xml", styles_xml());
    pkg.set_part("docProps/core.xml", core_xml());
    pkg.set_part("docProps/app.xml", app_xml());
    pkg.set_content_types(&ct);
    Ok(pkg)
}

fn sheet_xml(spec: &SheetSpec, strings: &mut Vec<String>) -> Result<XmlDocument, XlsxError> {
    let mut cells: Vec<(usize, usize, CellValue)> = Vec::new();
    for (k, v) in &spec.cells {
        let (row, col) = crate::cell::parse_cell_ref(k)?;
        cells.push((row, col, json_value(v)));
    }
    for (r, row) in spec.rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            cells.push((r + 1, c + 1, json_value(v)));
        }
    }
    cells.sort_by_key(|(r, c, _)| (*r, *c));

    let mut sheet_data = XmlNode::element("sheetData");
    let mut i = 0;
    while i < cells.len() {
        let row_n = cells[i].0;
        let mut row = XmlNode::element("row").with_attr("r", row_n.to_string());
        while i < cells.len() && cells[i].0 == row_n {
            let (r, c, ref val) = cells[i];
            row = row.with_child(encode_cell(&format_cell_ref(r, c), val, strings));
            i += 1;
        }
        sheet_data = sheet_data.with_child(row);
    }

    Ok(XmlDocument::new(
        XmlNode::element("worksheet")
            .with_attr("xmlns", ns::S)
            .with_attr("xmlns:r", ns::R)
            .with_child(sheet_data),
    ))
}

fn json_value(v: &serde_json::Value) -> CellValue {
    match v {
        serde_json::Value::Null => CellValue::Empty,
        serde_json::Value::Bool(b) => CellValue::Bool(*b),
        serde_json::Value::Number(n) => CellValue::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => {
            if let Some(f) = s.strip_prefix('=') {
                CellValue::Formula {
                    formula: f.to_string(),
                    cached: None,
                }
            } else {
                CellValue::Text(s.clone())
            }
        }
        serde_json::Value::Object(m) => {
            if let Some(f) = m.get("formula").and_then(|x| x.as_str()) {
                CellValue::Formula {
                    formula: f.to_string(),
                    cached: m.get("cached").and_then(|x| x.as_str()).map(str::to_string),
                }
            } else if let Some(t) = m.get("text").and_then(|x| x.as_str()) {
                CellValue::Text(t.to_string())
            } else {
                CellValue::Text(v.to_string())
            }
        }
        other => CellValue::Text(other.to_string()),
    }
}

fn styles_xml() -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="{s}">
  <fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>
  <fills count="1"><fill><patternFill patternType="none"/></fill></fills>
  <borders count="1"><border/></borders>
  <cellStyleXfs count="1"><xf/></cellStyleXfs>
  <cellXfs count="1"><xf xfId="0"/></cellXfs>
</styleSheet>"#,
        s = ns::S,
    )
    .into_bytes()
}

fn core_xml() -> Vec<u8> {
    let now = utc_now_iso();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="{cp}" xmlns:dc="{dc}" xmlns:dcterms="{dcterms}" xmlns:xsi="{xsi}">
  <dc:creator>blackline</dc:creator>
  <dcterms:created xsi:type="dcterms:W3CDTF">{now}</dcterms:created>
</cp:coreProperties>"#,
        cp = ns::CP,
        dc = ns::DC,
        dcterms = ns::DCTERMS,
        xsi = ns::XSI,
    )
    .into_bytes()
}

fn app_xml() -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="{ep}"><Application>blackline</Application></Properties>"#,
        ep = ns::EP,
    )
    .into_bytes()
}

/// Convenience: one sheet, row-major text grid.
pub fn from_rows(name: &str, rows: &[Vec<&str>]) -> Result<Package, XlsxError> {
    let spec = CreateSpec {
        sheets: vec![SheetSpec {
            name: name.into(),
            cells: serde_json::Map::new(),
            rows: rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|c| serde_json::Value::String((*c).into()))
                        .collect()
                })
                .collect(),
        }],
    };
    create(&spec)
}
