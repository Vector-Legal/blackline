//! Workbook façade.

use std::path::{Path, PathBuf};

use serde::Serialize;

use blackline_core::ns;
use blackline_core::package::Package;
use blackline_core::rels;
use blackline_core::xml::XmlNode;

use crate::cell::{format_cell_ref, parse_cell_ref};
use crate::create::{self, CreateSpec};
use crate::edit::{self, EditOp, EditOptions, EditReport};
use crate::error::XlsxError;
use crate::text::{self, SearchHit, SheetView};

/// A cell value.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    /// Empty.
    Empty,
    /// Number.
    Number(f64),
    /// Inline or resolved shared string.
    Text(String),
    /// Formula with optional cached value.
    Formula {
        /// Formula body (no leading `=`).
        formula: String,
        /// Cached computed value, if present.
        cached: Option<String>,
    },
    /// Boolean.
    Bool(bool),
}

/// One sheet listing.
#[derive(Debug, Clone, Serialize)]
pub struct SheetInfo {
    /// 1-based sheet index (workbook order).
    pub index: usize,
    /// Sheet name.
    pub name: String,
    /// Part name (`xl/worksheets/sheet1.xml`).
    pub part: String,
}

/// Workbook metrics.
#[derive(Debug, Clone, Serialize)]
pub struct XlsxInfo {
    /// Source file.
    pub file: String,
    /// Sheet count.
    pub sheets: usize,
    /// Sheet names.
    pub sheet_names: Vec<String>,
}

/// An open workbook.
pub struct Xlsx {
    pub(crate) pkg: Package,
    path: Option<PathBuf>,
}

impl Xlsx {
    /// Open from a path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, XlsxError> {
        let path = path.as_ref();
        Ok(Self {
            pkg: Package::open(path)?,
            path: Some(path.to_path_buf()),
        })
    }

    /// Open from bytes.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, XlsxError> {
        Ok(Self {
            pkg: Package::from_bytes(bytes.as_ref())?,
            path: None,
        })
    }

    /// Open from a package.
    pub fn from_package(pkg: Package) -> Self {
        Self { pkg, path: None }
    }

    /// Create from a spec.
    pub fn create(spec: &CreateSpec) -> Result<Self, XlsxError> {
        Ok(Self {
            pkg: create::create(spec)?,
            path: None,
        })
    }

    /// One sheet from a row-major text grid.
    pub fn from_rows(name: &str, rows: &[Vec<&str>]) -> Result<Self, XlsxError> {
        Ok(Self {
            pkg: create::from_rows(name, rows)?,
            path: None,
        })
    }

    /// The underlying package.
    pub fn package(&self) -> &Package {
        &self.pkg
    }

    /// Mutable package.
    pub fn package_mut(&mut self) -> &mut Package {
        &mut self.pkg
    }

    /// Sheet listing.
    pub fn sheets(&self) -> Result<Vec<SheetInfo>, XlsxError> {
        list_sheets(&self.pkg)
    }

    /// Numbered view of one sheet (or all sheets).
    pub fn view(&self, sheet: Option<&str>) -> Result<Vec<SheetView>, XlsxError> {
        text::view(&self.pkg, sheet)
    }

    /// Search cell text.
    pub fn find(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, XlsxError> {
        text::find(&self.pkg, query, limit)
    }

    /// Metrics.
    pub fn info(&self) -> Result<XlsxInfo, XlsxError> {
        let sheets = self.sheets()?;
        Ok(XlsxInfo {
            file: self
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            sheets: sheets.len(),
            sheet_names: sheets.into_iter().map(|s| s.name).collect(),
        })
    }

    /// Apply edit ops.
    pub fn edit(&mut self, ops: &[EditOp], opts: &EditOptions) -> Result<EditReport, XlsxError> {
        edit::apply(&mut self.pkg, ops, opts)
    }

    /// Read a cell.
    pub fn cell(&self, sheet: &str, cell: &str) -> Result<CellValue, XlsxError> {
        get_cell(&self.pkg, sheet, cell)
    }

    /// Health check.
    pub fn check(&self) -> Result<blackline_core::HealthReport, XlsxError> {
        Ok(blackline_core::check(&self.pkg)?)
    }

    /// Save.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), XlsxError> {
        self.pkg.save(path)?;
        Ok(())
    }

    /// Bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, XlsxError> {
        Ok(self.pkg.to_bytes()?)
    }
}

pub(crate) fn list_sheets(pkg: &Package) -> Result<Vec<SheetInfo>, XlsxError> {
    let wb = pkg.part_xml("xl/workbook.xml")?;
    let rels = pkg.rels_for("xl/workbook.xml")?;
    let mut out = Vec::new();
    for (i, node) in wb.root.find_all("sheet").into_iter().enumerate() {
        let name = node.get_attr("name").unwrap_or("Sheet").to_string();
        let rid = node.get_attr("id").unwrap_or("");
        let target = rels
            .by_id(rid)
            .map(|r| rels::resolve_target("xl/workbook.xml", &r.target))
            .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", i + 1));
        out.push(SheetInfo {
            index: i + 1,
            name,
            part: target,
        });
    }
    Ok(out)
}

pub(crate) fn sheet_by_name_or_index<'a>(
    sheets: &'a [SheetInfo],
    key: &str,
) -> Result<&'a SheetInfo, XlsxError> {
    if let Ok(n) = key.parse::<usize>() {
        return sheets
            .get(n.saturating_sub(1))
            .or_else(|| sheets.iter().find(|s| s.index == n))
            .ok_or_else(|| XlsxError::invalid(format!("sheet {key} not found")));
    }
    sheets
        .iter()
        .find(|s| s.name == key)
        .ok_or_else(|| XlsxError::invalid(format!("sheet {key} not found")))
}

pub(crate) fn shared_strings(pkg: &Package) -> Result<Vec<String>, XlsxError> {
    if !pkg.has_part("xl/sharedStrings.xml") {
        return Ok(Vec::new());
    }
    let doc = pkg.part_xml("xl/sharedStrings.xml")?;
    let mut out = Vec::new();
    for si in doc.root.find_all("si") {
        // Concatenate all t descendants (rich text).
        let mut s = String::new();
        for t in si.find_all("t") {
            s.push_str(&t.text_content());
        }
        out.push(s);
    }
    Ok(out)
}

pub(crate) fn set_shared_strings(pkg: &mut Package, strings: &[String]) -> Result<(), XlsxError> {
    let existed = pkg.has_part("xl/sharedStrings.xml");
    if existed {
        if shared_strings(pkg)? == strings {
            return Ok(());
        }
    } else if strings.is_empty() {
        let rels = pkg.rels_for("xl/workbook.xml")?;
        if rels
            .by_type(blackline_core::ns::rel::SHARED_STRINGS)
            .is_none()
        {
            return Ok(());
        }
    }

    let mut sst = XmlNode::element("sst")
        .with_attr("xmlns", ns::S)
        .with_attr("count", strings.len().to_string())
        .with_attr("uniqueCount", strings.len().to_string());
    for s in strings {
        sst = sst.with_child(
            XmlNode::element("si").with_child(XmlNode::element("t").with_text(s.as_str())),
        );
    }
    pkg.set_part_xml(
        "xl/sharedStrings.xml",
        &blackline_core::XmlDocument::new(sst),
    );

    if !existed {
        let mut ct = pkg.content_types()?;
        ct.ensure_override(
            "xl/sharedStrings.xml",
            blackline_core::ns::content::SHARED_STRINGS,
        );
        pkg.set_content_types(&ct);
        let mut rels = pkg.rels_for("xl/workbook.xml")?;
        if rels
            .by_type(blackline_core::ns::rel::SHARED_STRINGS)
            .is_none()
        {
            rels.add(blackline_core::Relationship::internal(
                String::new(),
                blackline_core::ns::rel::SHARED_STRINGS,
                "sharedStrings.xml",
            ));
            pkg.set_rels_for("xl/workbook.xml", &rels);
        }
    }
    Ok(())
}

fn get_cell(pkg: &Package, sheet: &str, cell: &str) -> Result<CellValue, XlsxError> {
    let sheets = list_sheets(pkg)?;
    let info = sheet_by_name_or_index(&sheets, sheet)?;
    let doc = pkg.part_xml(&info.part)?;
    let strings = shared_strings(pkg)?;
    for (r, c) in sheet_cells(&doc) {
        if r == cell {
            return Ok(decode_cell(c, &strings));
        }
    }
    let _ = parse_cell_ref(cell)?;
    Ok(CellValue::Empty)
}

/// Cells in sheet order, filling in omitted SpreadsheetML `r` attributes.
///
/// A missing row `@r` is the next row after the previous one (starting at 1).
/// A missing cell `@r` is the next column on that row (starting at A).
pub(crate) fn sheet_cells(doc: &blackline_core::XmlDocument) -> Vec<(String, &XmlNode)> {
    let mut out = Vec::new();
    let Some(sheet_data) = doc.root.find_child("sheetData") else {
        return out;
    };
    let mut next_row = 1usize;
    for row in sheet_data.children() {
        if !row.is_element_with_local_name("row") {
            continue;
        }
        let row_n = row
            .get_attr("r")
            .and_then(|s| s.parse().ok())
            .unwrap_or(next_row);
        next_row = row_n + 1;
        let mut next_col = 1usize;
        for c in row.children() {
            if !c.is_element_with_local_name("c") {
                continue;
            }
            let pref = if let Some(explicit) = c.get_attr("r") {
                if let Ok((_, col)) = parse_cell_ref(explicit) {
                    next_col = col + 1;
                }
                explicit.to_string()
            } else {
                let implied = format_cell_ref(row_n, next_col);
                next_col += 1;
                implied
            };
            out.push((pref, c));
        }
    }
    out
}

pub(crate) fn decode_cell(c: &XmlNode, strings: &[String]) -> CellValue {
    if let Some(f) = c.find_child("f") {
        let cached = c.find_child("v").map(|v| v.text_content());
        return CellValue::Formula {
            formula: f.text_content(),
            cached,
        };
    }
    let v = c
        .find_child("v")
        .map(|n| n.text_content())
        .unwrap_or_default();
    match c.get_attr("t") {
        Some("s") => {
            let i: usize = v.parse().unwrap_or(0);
            CellValue::Text(strings.get(i).cloned().unwrap_or_default())
        }
        Some("inlineStr") => {
            let t = c
                .find_all("t")
                .into_iter()
                .map(|n| n.text_content())
                .collect();
            CellValue::Text(t)
        }
        Some("b") => CellValue::Bool(v == "1" || v.eq_ignore_ascii_case("true")),
        Some("str") => CellValue::Text(v),
        _ => {
            if v.is_empty() {
                CellValue::Empty
            } else if let Ok(n) = v.parse::<f64>() {
                CellValue::Number(n)
            } else {
                CellValue::Text(v)
            }
        }
    }
}

pub(crate) fn encode_cell(cell_ref: &str, value: &CellValue, strings: &mut Vec<String>) -> XmlNode {
    let mut c = XmlNode::element("c").with_attr("r", cell_ref);
    match value {
        CellValue::Empty => c,
        CellValue::Number(n) => c.with_child(XmlNode::element("v").with_text(n.to_string())),
        CellValue::Text(s) => {
            let idx = if let Some(i) = strings.iter().position(|x| x == s) {
                i
            } else {
                strings.push(s.clone());
                strings.len() - 1
            };
            c.set_attr("t", "s");
            c.with_child(XmlNode::element("v").with_text(idx.to_string()))
        }
        CellValue::Formula { formula, cached } => {
            let mut n = c.with_child(XmlNode::element("f").with_text(formula.as_str()));
            if let Some(v) = cached {
                n = n.with_child(XmlNode::element("v").with_text(v.as_str()));
            }
            n
        }
        CellValue::Bool(b) => {
            c.set_attr("t", "b");
            c.with_child(XmlNode::element("v").with_text(if *b { "1" } else { "0" }))
        }
    }
}

#[allow(dead_code)]
fn _fmt(r: usize, c: usize) -> String {
    format_cell_ref(r, c)
}
