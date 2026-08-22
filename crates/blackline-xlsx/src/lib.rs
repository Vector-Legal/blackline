//! # blackline-xlsx
//!
//! Read, create, and edit XLSX workbooks. Cells, shared strings, and
//! sheets are ordinary OPC parts plus SpreadsheetML.

mod cell;
mod create;
mod edit;
mod error;
mod text;
mod workbook;

pub use cell::{col_letter, col_number, parse_cell_ref};
pub use create::{from_rows, CreateSpec, SheetSpec};
pub use edit::{EditOp, EditOptions, EditReport};
pub use error::XlsxError;
pub use text::{SearchHit, SheetView};
pub use workbook::{CellValue, SheetInfo, Xlsx, XlsxInfo};
