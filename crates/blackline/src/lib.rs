//! `blackline` — an agent-first CLI for DOCX / XLSX / PPTX.
//!
//! Canonical grammar: `blackline <format> <verb> [args]` plus `ai`,
//! `unpack`, `pack`, and `xml`.

mod cmd_ai;
mod cmd_docx;
mod cmd_pptx;
mod cmd_track;
mod cmd_xlsx;
mod cmd_xml;
mod fixtures;
mod util;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "blackline",
    version,
    about = "Unopinionated Office Open XML toolkit: read, search, edit, redline, create, and validate DOCX / XLSX / PPTX",
    after_help = "Conventions:\n  \
        - Outputs: -o/--output PATH or --in-place\n  \
        - JSON inputs (--ops/--spec): inline JSON, @file.json, or - for stdin\n  \
        - Ranges: --from N --to N (1-based, inclusive)\n  \
        - Tracked changes and comments require --author (or BLACKLINE_AUTHOR)\n  \
        - Exit codes: 0 success, 1 operation failure, 2 usage error\n\n\
        XML escape hatch:\n  \
        blackline xml get FILE PART [--path body/p[0]/r[0]/t]\n  \
        blackline xml eval FILE PART 'count(//ins)'\n  \
        blackline xml select FILE PART '//p[0]'\n  \
        blackline xml edit FILE --ops '[{\"part\":\"word/document.xml\",\"action\":\"set_text\",\"path\":\"…\",\"text\":\"…\"}]' -o OUT\n  \
        blackline xml patch FILE --part word/document.xml --ops '[{\"op\":\"remove\",\"sel\":\"//ins\"}]' -o OUT\n  \
        blackline xml update FILE --part word/document.xml --ops 'delete node //ins' -o OUT\n\n\
        Track / redline module:\n  \
        blackline track apply FILE --ops JSON -o OUT [--author NAME]\n  \
        blackline track redline ORIGINAL REVISED -o OUT --author NAME\n  \
        blackline track changes FILE [--author NAME]\n  \
        blackline track comments FILE [--author NAME]\n  \
        blackline track settle FILE --accept|--reject [--author NAME] -o OUT\n\n\
        Natural language (local Kalosm model; rebuild with --features kalosm):\n  \
        blackline ai FILE INSTRUCTION -o OUT --author NAME\n\n\
        Generate a synthetic test corpus:\n  \
        blackline fixtures DIR"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Word documents
    Docx {
        #[command(subcommand)]
        cmd: cmd_docx::DocxCmd,
    },
    /// Excel workbooks
    Xlsx {
        #[command(subcommand)]
        cmd: cmd_xlsx::XlsxCmd,
    },
    /// PowerPoint presentations
    Pptx {
        #[command(subcommand)]
        cmd: cmd_pptx::PptxCmd,
    },
    /// Raw XML tree operations and formulas on a package part
    Xml {
        #[command(subcommand)]
        cmd: cmd_xml::XmlCmd,
    },
    /// Multi-author track changes / redline (Word)
    Track {
        #[command(subcommand)]
        cmd: cmd_track::TrackCmd,
    },
    /// Local AI: a prompt becomes native OOXML edits
    #[command(about = blackline_ai::ABOUT, after_help = blackline_ai::AFTER_HELP)]
    Ai(blackline_ai::AiArgs),
    /// Unpack an OOXML package into pretty-printed XML
    Unpack {
        /// Package
        input_file: PathBuf,
        /// Directory
        output_directory: PathBuf,
    },
    /// Pack a directory back into an OOXML package
    Pack {
        /// Directory
        input_directory: PathBuf,
        /// Package
        output_file: PathBuf,
    },
    /// Write a battery of synthetic DOCX / XLSX / PPTX files
    Fixtures {
        /// Output directory
        output_directory: PathBuf,
    },
}

/// Parse argv and run. Returns the process exit code.
pub fn run() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Docx { cmd } => cmd_docx::run(cmd),
        Commands::Xlsx { cmd } => cmd_xlsx::run(cmd),
        Commands::Pptx { cmd } => cmd_pptx::run(cmd),
        Commands::Xml { cmd } => cmd_xml::run(cmd),
        Commands::Track { cmd } => cmd_track::run(cmd),
        Commands::Ai(args) => cmd_ai::run(args),
        Commands::Unpack {
            input_file,
            output_directory,
        } => cmd_xml::unpack(&input_file, &output_directory),
        Commands::Pack {
            input_directory,
            output_file,
        } => cmd_xml::pack(&input_directory, &output_file),
        Commands::Fixtures { output_directory } => fixtures::run(&output_directory),
    };
    match result {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code.clamp(0, 255) as u8),
        Err(e) if e.starts_with("usage:") => {
            eprintln!("{}", e.strip_prefix("usage: ").unwrap_or(&e).trim());
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
