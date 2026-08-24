//! Thin shim over [`blackline_llm::run`].

fn main() -> std::process::ExitCode {
    blackline_llm::run()
}
