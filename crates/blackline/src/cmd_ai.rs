//! `blackline ai FILE INSTRUCTION` — same pipeline as `blackline-llm`.

use blackline_llm::LlmArgs;

/// Run the shared AI pipeline and map its errors onto this CLI's exit codes.
pub fn run(args: LlmArgs) -> Result<i32, String> {
    match blackline_llm::run_args(args) {
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
