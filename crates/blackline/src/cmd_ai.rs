//! `blackline ai|llm FILE INSTRUCTION`.

use crate::ai::{run_args, AiArgs};

/// Run the AI pipeline and map its errors onto this CLI's exit codes.
pub fn run(args: AiArgs) -> Result<i32, String> {
    match run_args(args) {
        Ok(()) => Ok(0),
        Err(e) if e.is_usage() => {
            let msg = e.to_string();
            if msg.starts_with("usage:") {
                Err(msg)
            } else {
                Err(format!("usage: {msg}"))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}
