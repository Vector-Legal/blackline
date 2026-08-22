//! # blackline-core
//!
//! Shared foundations for the `blackline` Office toolkit:
//!
//! - [`package`] — in-memory OPC (ZIP) packages: parts, lossless untouched
//!   bytes, save / extract / rebuild from a directory.
//! - [`xml`] — a small XML DOM tuned for OOXML editing.
//! - [`formula`] — query layer: RFC 5261 / XPath-subset selectors and
//!   formulas (`count(//ins)`, `text(//p[0])`) evaluated against [`XmlNode`].
//! - [`patch`] — RFC 5261 `add` / `replace` / `remove`, compiled to [`TreeOp`].
//! - [`update`] — XQuery Update verbs, compiled to [`TreeOp`].
//! - [`tree`] — path-addressed XML mutations (`TreeOp`) that every
//!   higher layer compiles down to.
//! - [`rels`] / [`ctypes`] — relationships and `[Content_Types].xml`.
//! - [`mod@diff`] / [`textutil`] — tokenization, sentence splits, LCS.
//! - [`validate`] — structured package health checks.

pub mod ctypes;
pub mod diff;
pub mod error;
pub mod formula;
pub mod ns;
pub mod opc;
pub mod package;
pub mod patch;
pub mod rels;
pub mod textutil;
pub mod time;
pub mod tree;
pub mod update;
pub mod validate;
pub mod xml;

pub use ctypes::ContentTypes;
pub use diff::{diff, diff_minimal, mark, mark_minimal, minimize_hunks, DiffHunk, Granularity};
pub use error::CoreError;
pub use formula::{
    eval_str, select_str, Formula, Match as FormulaMatch, Selector, Value as FormulaValue,
};
pub use package::Package;
pub use patch::{parse_ops as parse_patch_ops, PatchOp};
pub use rels::{Relationship, Relationships};
pub use tree::{NodePath, TreeOp, TreeOpReport};
pub use update::{parse_ops as parse_update_ops, UpdateOp};
pub use validate::{check, HealthReport};
pub use xml::{XmlDocument, XmlNode};
