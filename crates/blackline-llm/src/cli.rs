//! `blackline-llm FILE INSTRUCTION` — same conventions as `blackline`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use serde::Serialize;

use crate::apply::{self, ApplyOptions, ApplyReport};
use crate::error::LlmError;
use crate::model::{ModelId, DEFAULT_MODEL};
use crate::plan::{Completer, Plan};
use crate::view::DocumentView;

/// Natural-language frontend for blackline.
#[derive(Parser, Debug)]
#[command(
    name = "blackline-llm",
    version,
    about = "Local LLM that drives blackline: a prompt becomes native OOXML edits",
    after_help = "The model never writes OOXML. It emits a small op list; blackline applies it.\n\
        DOCX ops become Word tracked changes (pass --author). XLSX and PPTX are silent edits.\n\n\
        Default model is quantized Phi-3.5 mini (Kalosm). Override with --model.\n\
        First run downloads the GGUF into the Kalosm cache.\n\n\
        Examples:\n  \
        blackline-llm contract.docx \"change thirty days to sixty days\" -o out.docx --author \"Jane Doe\"\n  \
        blackline-llm model.xlsx \"set B2 to 42\" --in-place --model llama3.2-3b\n  \
        blackline-llm deck.pptx \"set the title to Q3\" -o out.pptx --model ./phi.gguf\n  \
        blackline-llm contract.docx \"flag the indemnity clause\" --dry-run --json --author Jane"
)]
pub struct Cli {
    /// DOCX / XLSX / PPTX file
    pub file: PathBuf,
    /// Natural-language instruction. `@path` reads a file; `-` reads stdin.
    pub instruction: String,
    /// Output path
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Overwrite the input
    #[arg(long)]
    pub in_place: bool,
    /// Author for tracked changes (or BLACKLINE_AUTHOR)
    #[arg(long)]
    pub author: Option<String>,
    /// Model preset or a .gguf path (default: phi-3.5)
    #[arg(long, default_value = DEFAULT_MODEL)]
    pub model: String,
    /// Don't write
    #[arg(long)]
    pub dry_run: bool,
    /// JSON report (plan + apply)
    #[arg(long)]
    pub json: bool,
    /// DOCX: edit silently instead of leaving a redline
    #[arg(long)]
    pub no_track: bool,
    /// Best-effort apply
    #[arg(long)]
    pub lenient: bool,
    /// char | word | sentence
    #[arg(long, default_value = "word")]
    pub granularity: String,
    /// First view index (1-based)
    #[arg(long)]
    pub from: Option<usize>,
    /// Last view index
    #[arg(long)]
    pub to: Option<usize>,
    /// XLSX sheet name or 1-based index
    #[arg(long)]
    pub sheet: Option<String>,
    /// Log model load
    #[arg(long)]
    pub verbose: bool,
}

/// JSON document written by `--json`.
#[derive(Debug, Serialize)]
pub struct LlmReport {
    /// `docx` / `xlsx` / `pptx`.
    pub format: crate::format::Format,
    /// Preset name or GGUF path.
    pub model: String,
    /// Ops the model produced.
    pub plan: Plan,
    /// Apply result.
    pub apply: ApplyReport,
    /// Written path, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

/// Parse argv and run. Returns the process exit code.
pub fn run() -> ExitCode {
    let cli = Cli::parse();
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio")
        .block_on(run_cli(cli))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is_usage() => {
            let msg = e.to_string();
            let msg = msg.strip_prefix("usage: ").unwrap_or(&msg);
            eprintln!("{msg}");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

pub(crate) async fn run_cli(cli: Cli) -> Result<(), LlmError> {
    let instruction = read_instruction(&cli.instruction)?;
    if instruction.trim().is_empty() {
        return Err(LlmError::usage("instruction is empty".to_string()));
    }
    let output =
        apply::resolve_output(&cli.file, cli.output.as_deref(), cli.in_place, cli.dry_run)?;
    let granularity = parse_granularity(&cli.granularity)?;
    let author = resolve_author(cli.author.as_deref())?;
    let model_id = ModelId::parse(&cli.model)?;

    let view = DocumentView::open(&cli.file, cli.from, cli.to, cli.sheet.as_deref())?;

    if view.format == crate::format::Format::Docx && !cli.no_track && author.is_none() {
        return Err(LlmError::usage(
            "author required: tracked changes and comments must carry an explicit author \
             (pass --author or set BLACKLINE_AUTHOR)"
                .to_string(),
        ));
    }

    let plan = complete_with_model(&model_id, &view, &instruction, cli.verbose).await?;

    let opts = ApplyOptions {
        author,
        granularity,
        no_track: cli.no_track,
        lenient: cli.lenient,
        dry_run: cli.dry_run,
    };
    let apply_report = apply::apply(&cli.file, output.as_deref(), &plan, &opts)?;

    let report = LlmReport {
        format: view.format,
        model: model_id.as_str(),
        plan,
        apply: apply_report,
        output: output.map(|p| p.display().to_string()),
    };
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| LlmError::Io(e.to_string()))?
        );
    } else {
        print_human(&report);
    }
    Ok(())
}

async fn complete_with_model(
    id: &ModelId,
    view: &DocumentView,
    instruction: &str,
    verbose: bool,
) -> Result<Plan, LlmError> {
    #[cfg(feature = "kalosm")]
    {
        let completer = crate::model::load(id, verbose).await?;
        return completer.complete(view, instruction).await;
    }
    #[cfg(not(feature = "kalosm"))]
    {
        let _ = (id, view, instruction, verbose);
        Err(LlmError::usage(
            "blackline-llm was built without Kalosm. Install with:\n  \
             cargo install blackline-llm --features kalosm\n  \
             cargo install blackline-llm --features kalosm,metal   # Apple Silicon\n  \
             cargo install blackline-llm --features kalosm,cuda    # NVIDIA"
                .to_string(),
        ))
    }
}

fn read_instruction(raw: &str) -> Result<String, LlmError> {
    if raw == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| LlmError::Io(format!("failed to read stdin: {e}")))?;
        return Ok(buf);
    }
    if let Some(path) = raw.strip_prefix('@') {
        return std::fs::read_to_string(path)
            .map_err(|e| LlmError::Io(format!("failed to read {path}: {e}")));
    }
    Ok(raw.to_string())
}

fn parse_granularity(raw: &str) -> Result<blackline_docx::Granularity, LlmError> {
    match raw {
        "char" => Ok(blackline_docx::Granularity::Char),
        "word" => Ok(blackline_docx::Granularity::Word),
        "sentence" => Ok(blackline_docx::Granularity::Sentence),
        other => Err(LlmError::usage(format!(
            "unknown --granularity {other:?} (char | word | sentence)"
        ))),
    }
}

fn resolve_author(flag: Option<&str>) -> Result<Option<String>, LlmError> {
    if let Some(a) = flag {
        if !a.trim().is_empty() {
            return Ok(Some(a.to_string()));
        }
    }
    if let Ok(a) = std::env::var("BLACKLINE_AUTHOR") {
        if !a.trim().is_empty() {
            return Ok(Some(a));
        }
    }
    Ok(None)
}

fn print_human(report: &LlmReport) {
    eprintln!(
        "{}  model={}  {} op(s)  applied={}  failed={}  {}",
        report.format,
        report.model,
        report.plan.ops.len(),
        report.apply.applied,
        report.apply.failed,
        report.apply.mode
    );
    for op in &report.apply.ops {
        eprintln!("  [{}] {} {} — {}", op.index, op.status, op.op, op.detail);
    }
    if let Some(path) = &report.output {
        eprintln!("wrote {path}");
    }
}

/// Library entry used by tests: skip Kalosm, inject a completer.
pub async fn run_with_completer<C: Completer>(
    file: &std::path::Path,
    instruction: &str,
    output: Option<&std::path::Path>,
    opts: ApplyOptions,
    completer: &C,
) -> Result<LlmReport, LlmError> {
    let view = DocumentView::open(file, None, None, None)?;
    let plan = completer.complete(&view, instruction).await?;
    let apply_report = apply::apply(file, output, &plan, &opts)?;
    Ok(LlmReport {
        format: view.format,
        model: "static".into(),
        plan,
        apply: apply_report,
        output: output.map(|p| p.display().to_string()),
    })
}
