//! # blackline-pptx
//!
//! Read, create, and edit PPTX presentations. Slides are OPC parts;
//! text lives in DrawingML `a:t` elements.

mod create;
mod edit;
mod error;
mod presentation;

pub use create::{CreateSpec, SlideSpec};
pub use edit::{EditOp, EditOptions, EditReport};
pub use error::PptxError;
pub use presentation::{Pptx, PptxInfo, SearchHit, SlideInfo, SlideView};
