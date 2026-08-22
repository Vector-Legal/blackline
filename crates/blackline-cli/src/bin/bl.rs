//! Thin shim over [`blackline_cli::run`]. See `src/lib.rs` for the CLI itself.

fn main() -> std::process::ExitCode {
    blackline_cli::run()
}
