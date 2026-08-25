//! Local AI frontend: a prompt becomes blackline ops. The model never
//! writes XML.
#![allow(dead_code)]
//!
//! [`plan::Completer`] is the only extra abstraction. Production uses Kalosm
//! to emit a [`plan::Plan`] (JSON, then serde). Tests inject
//! [`plan::StaticCompleter`].

mod apply;
mod cache;
mod cli;
mod error;
mod format;
mod model;
mod plan;
mod snap;
mod view;

#[cfg(all(feature = "kalosm", feature = "metal", target_os = "macos"))]
mod metal_infer;

#[cfg(test)]
mod pipeline;

pub(crate) use cli::{run_args, AiArgs, ABOUT, AFTER_HELP};
