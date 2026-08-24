//! # blackline-ai
//!
//! Natural-language frontend for blackline. A local Kalosm model emits a
//! small op list; blackline applies it as native OOXML. The model never
//! writes XML.
//!
//! [`Completer`] is the only extra abstraction. Production uses Kalosm
//! constrained generation into [`Plan`]. Tests inject [`StaticCompleter`].

mod apply;
mod cli;
mod error;
mod format;
mod model;
mod plan;
mod view;

pub use apply::{apply, ApplyOptions, ApplyReport, OpStat};
pub use cli::{
    exit_from, run, run_args, run_with_completer, Cli, AiArgs, AiReport, ABOUT, AFTER_HELP,
};
pub use error::AiError;
pub use format::Format;
pub use model::{ModelId, DEFAULT_MODEL};
pub use plan::{system_prompt, user_prompt, Completer, Op, Plan, Position, StaticCompleter};
pub use view::{DocumentView, CONTEXT_CHARS};
