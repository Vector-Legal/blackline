//! Thin shim over [`blackline_ai::run`].

fn main() -> std::process::ExitCode {
    blackline_ai::run()
}
