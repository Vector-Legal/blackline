//! Local AI frontend: a prompt becomes blackline ops. The model never
//! writes XML.
//!
//! [`plan::Completer`] emits a [`plan::Plan`]. `snap::snap_plan` rewrites
//! those ops onto text that exists in the paragraph. Production uses
//! Kalosm / llama.cpp; tests inject [`plan::StaticCompleter`].
//!
//! Helpers used only with `--features kalosm` (or Metal on macOS) stay
//! in this module so `--help` and unit tests do not need a cfg maze.
#![allow(dead_code)]

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
