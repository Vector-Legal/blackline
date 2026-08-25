//! `bl ai FILE INSTRUCTION` — a prompt becomes native OOXML edits.

use std::path::PathBuf;

use clap::Args;
use serde::Serialize;

use super::apply::{self, ApplyOptions, ApplyReport};
use super::error::AiError;
use super::model::{ModelId, DEFAULT_MODEL};
use super::plan::{Completer, Plan};
use super::view::DocumentView;

/// One-line about text for `bl ai`.
pub const ABOUT: &str = "Local AI that drives blackline: a prompt becomes native OOXML edits";

/// Longer help shown after the flag list.
pub const AFTER_HELP: &str = "The model never writes OOXML. It emits a small op list; blackline applies it.\n\
        DOCX ops become Word tracked changes (pass --author). XLSX and PPTX are silent edits.\n\n\
        Default model is quantized Phi-3 mini 4k (Kalosm). Override with --model.\n\
        First run downloads the GGUF into the Kalosm cache. On Apple Silicon\n\
        (`--features metal`) inference is llama.cpp Metal (all layers on the\n\
        GPU). Elsewhere Kalosm owns the tensors and drops them after the plan.\n\
        `bl ai --clear-cache` deletes the GGUFs.\n\n\
        You do not pass paragraph indexes. Phrases in the instruction select\n\
        the view (`change thirty days to sixty days` looks up that text;\n\
        `change title to …` is the first paragraph). `every paragraph` /\n\
        `throughout the document` walks the file in chunks. `--from` / `--to`\n\
        is an optional override, same index space as `bl docx view`.\n\n\
        Examples:\n  \
        bl ai contract.docx \"change thirty days to sixty days\" -o out.docx --author \"Jane Doe\"\n  \
        bl ai model.xlsx \"set B2 to 42\" --in-place --model llama3.2-1b\n  \
        bl ai deck.pptx \"set the title to Q3\" -o out.pptx --model ./phi.gguf\n  \
        bl ai contract.docx \"flag the indemnity clause\" --dry-run --json --author Jane\n  \
        bl ai --clear-cache";

/// Flags for `bl ai`.
#[derive(Args, Debug)]
pub struct AiArgs {
    /// DOCX / XLSX / PPTX file
    #[arg(required_unless_present = "clear_cache")]
    pub file: Option<PathBuf>,
    /// Natural-language instruction. `@path` reads a file; `-` reads stdin.
    #[arg(required_unless_present = "clear_cache")]
    pub instruction: Option<String>,
    /// Output path
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Overwrite the input
    #[arg(long)]
    pub in_place: bool,
    /// Author for tracked changes (or BLACKLINE_AUTHOR)
    #[arg(long)]
    pub author: Option<String>,
    /// Model preset or a .gguf path (default: phi-3)
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
    /// First view index (1-based). Optional. Default: phrases from the instruction.
    #[arg(long)]
    pub from: Option<usize>,
    /// Last view index. Optional. Default: phrases from the instruction.
    #[arg(long)]
    pub to: Option<usize>,
    /// XLSX sheet name or 1-based index
    #[arg(long)]
    pub sheet: Option<String>,
    /// Log model load
    #[arg(long)]
    pub verbose: bool,
    /// Delete downloaded GGUFs from the Kalosm cache
    #[arg(long)]
    pub clear_cache: bool,
}

/// JSON document written by `--json`.
#[derive(Debug, Serialize)]
pub struct AiReport {
    /// `docx` / `xlsx` / `pptx`.
    pub format: super::format::Format,
    /// Preset name or GGUF path.
    pub model: String,
    /// Ops the model produced.
    pub plan: Plan,
    /// Apply result.
    pub apply: ApplyReport,
    /// Written path, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Present when `--clear-cache` ran after the edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<super::cache::ClearCacheReport>,
}

/// Run a parsed `bl ai` argument set.
pub fn run_args(args: AiArgs) -> Result<(), AiError> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio")
        .block_on(run_cli(args))
}

async fn run_cli(cli: AiArgs) -> Result<(), AiError> {
    match (&cli.file, &cli.instruction, cli.clear_cache) {
        (None, None, true) => return print_clear(cli.json),
        (Some(_), Some(_), _) => {}
        _ => {
            return Err(AiError::usage(
                "pass FILE INSTRUCTION, or --clear-cache with no file".to_string(),
            ));
        }
    }

    let file = cli.file.as_deref().expect("file present");
    let instruction = read_instruction(cli.instruction.as_deref().expect("instruction present"))?;
    if instruction.trim().is_empty() {
        return Err(AiError::usage("instruction is empty".to_string()));
    }
    let output = apply::resolve_output(file, cli.output.as_deref(), cli.in_place, cli.dry_run)?;
    let granularity = parse_granularity(&cli.granularity)?;
    let author = resolve_author(cli.author.as_deref())?;
    let model_id = ModelId::parse(&cli.model)?;

    let views = DocumentView::windows(
        file,
        cli.from,
        cli.to,
        cli.sheet.as_deref(),
        Some(instruction.as_str()),
    )?;
    let view_lines: usize = views.iter().map(|v| v.lines.len()).sum();
    let prompt_chars: usize = views.iter().map(DocumentView::prompt_chars).sum();
    let window = views
        .first()
        .map(|v| v.window.as_str())
        .filter(|s| !s.is_empty());
    eprintln!(
        "view {} line(s) in {} chunk(s)  {} prompt chars{}",
        view_lines,
        views.len(),
        prompt_chars,
        window.map(|w| format!("  {w}")).unwrap_or_default()
    );

    let format = views
        .first()
        .expect("windows always returns at least one view")
        .format;
    if format == super::format::Format::Docx && !cli.no_track && author.is_none() {
        return Err(AiError::usage(
            "author required: tracked changes and comments must carry an explicit author \
             (pass --author or set BLACKLINE_AUTHOR)"
                .to_string(),
        ));
    }

    let plan = complete_with_model(&model_id, &views, &instruction, cli.verbose).await?;

    let opts = ApplyOptions {
        author,
        granularity,
        no_track: cli.no_track,
        lenient: cli.lenient,
        dry_run: cli.dry_run,
    };
    let apply_report = apply::apply(file, output.as_deref(), &plan, &opts)?;
    let cache = if cli.clear_cache {
        Some(super::cache::clear_cache()?)
    } else {
        None
    };

    let report = AiReport {
        format,
        model: model_id.as_str(),
        plan,
        apply: apply_report,
        output: output.map(|p| p.display().to_string()),
        cache,
    };
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| AiError::Io(e.to_string()))?
        );
    } else {
        print_human(&report);
        if let Some(cache) = &report.cache {
            eprintln!("{}", super::cache::format_clear_report(cache));
        }
    }
    Ok(())
}

fn print_clear(json: bool) -> Result<(), AiError> {
    let report = super::cache::clear_cache()?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| AiError::Io(e.to_string()))?
        );
    } else {
        eprintln!("{}", super::cache::format_clear_report(&report));
    }
    Ok(())
}

async fn complete_with_model(
    id: &ModelId,
    views: &[DocumentView],
    instruction: &str,
    verbose: bool,
) -> Result<Plan, AiError> {
    #[cfg(feature = "kalosm")]
    {
        let completer = super::model::load(id, verbose).await?;
        let plan = completer.complete_views(views, instruction).await?;
        // Drop the runtime (llama.cpp context or Kalosm worker) before
        // the package write. The GGUF on disk is unchanged; use
        // `--clear-cache`.
        drop(completer);
        if verbose {
            eprintln!("unloaded model");
        }
        Ok(plan)
    }
    #[cfg(not(feature = "kalosm"))]
    {
        let _ = (id, views, instruction, verbose);
        Err(AiError::usage(
            "this binary was built without Kalosm. Rebuild with --features kalosm:\n  \
             cargo install blackline --features kalosm\n  \
             cargo install blackline --features kalosm,metal   # Apple Silicon\n  \
             cargo install blackline --features kalosm,cuda    # NVIDIA"
                .to_string(),
        ))
    }
}

fn read_instruction(raw: &str) -> Result<String, AiError> {
    if raw == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| AiError::Io(format!("failed to read stdin: {e}")))?;
        return Ok(buf);
    }
    if let Some(path) = raw.strip_prefix('@') {
        return std::fs::read_to_string(path)
            .map_err(|e| AiError::Io(format!("failed to read {path}: {e}")));
    }
    Ok(raw.to_string())
}

fn parse_granularity(raw: &str) -> Result<blackline_docx::Granularity, AiError> {
    match raw {
        "char" => Ok(blackline_docx::Granularity::Char),
        "word" => Ok(blackline_docx::Granularity::Word),
        "sentence" => Ok(blackline_docx::Granularity::Sentence),
        other => Err(AiError::usage(format!(
            "unknown --granularity {other:?} (char | word | sentence)"
        ))),
    }
}

fn resolve_author(flag: Option<&str>) -> Result<Option<String>, AiError> {
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

fn print_human(report: &AiReport) {
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
) -> Result<AiReport, AiError> {
    let view = DocumentView::open(file, None, None, None, Some(instruction))?;
    let plan = completer.complete(&view, instruction).await?;
    let apply_report = apply::apply(file, output, &plan, &opts)?;
    Ok(AiReport {
        format: view.format,
        model: "static".into(),
        plan,
        apply: apply_report,
        output: output.map(|p| p.display().to_string()),
        cache: None,
    })
}
