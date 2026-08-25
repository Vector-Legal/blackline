//! Local AI frontend: a prompt becomes blackline ops. The model never
//! writes XML.
#![allow(dead_code)]
//!
//! [`plan::Completer`] is the only extra abstraction. Production uses Kalosm
//! constrained generation into [`plan::Plan`]. Tests inject
//! [`plan::StaticCompleter`].

mod apply;
mod cache;
mod cli;
mod error;
mod format;
mod model;
mod plan;
mod view;

#[cfg(test)]
mod pipeline;

pub(crate) use cli::{run_args, AiArgs, ABOUT, AFTER_HELP};
