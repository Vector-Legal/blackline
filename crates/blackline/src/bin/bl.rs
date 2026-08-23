//! Thin shim over [`blackline::run`]. See `src/lib.rs` for the CLI itself.

fn main() -> std::process::ExitCode {
    blackline::run()
}
