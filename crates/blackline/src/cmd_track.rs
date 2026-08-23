//! `blackline track …` — multi-author redline module on top of the CLI.

use std::path::PathBuf;

use clap::Subcommand;

use blackline_docx::{track_redline, Docx, Granularity, TrackOp};

use crate::util::{atomic_write, print_json, read_json_arg, resolve_author, resolve_output};

/// Track / redline verbs.
#[derive(Subcommand)]
pub enum TrackCmd {
    /// Apply a JSON track recipe (replace / insert / delete / comment / settle)
    Apply {
        /// DOCX file
        file: PathBuf,
        /// Ops JSON, @file, or -
        #[arg(long)]
        ops: String,
        /// Output
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Overwrite input
        #[arg(long)]
        in_place: bool,
        /// Default author for ops that omit one
        #[arg(long)]
        author: Option<String>,
        /// Default ISO date for ops that omit one
        #[arg(long)]
        date: Option<String>,
        /// char | word | sentence
        #[arg(long, default_value = "word")]
        granularity: String,
        /// Best-effort
        #[arg(long)]
        lenient: bool,
        /// Don't write
        #[arg(long)]
        dry_run: bool,
        /// JSON report
        #[arg(long)]
        json: bool,
        /// Story part: `header`, `footer`, or `word/header2.xml`
        #[arg(long)]
        part: Option<String>,
    },
    /// Compare two documents into a minimized redline
    Redline {
        /// Original
        original: PathBuf,
        /// Revised
        revised: PathBuf,
        /// Output
        #[arg(short, long)]
        output: PathBuf,
        /// Author
        #[arg(long)]
        author: Option<String>,
        /// Granularity
        #[arg(long, default_value = "word")]
        granularity: String,
    },
    /// List tracked changes (always JSON)
    Changes {
        /// File
        file: PathBuf,
        /// Restrict to this author
        #[arg(long)]
        author: Option<String>,
    },
    /// List comments (always JSON)
    Comments {
        /// File
        file: PathBuf,
        /// Restrict to this author
        #[arg(long)]
        author: Option<String>,
    },
    /// Accept or reject tracked changes
    Settle {
        /// File
        file: PathBuf,
        /// Accept insertions, drop deletions
        #[arg(long, group = "settle")]
        accept: bool,
        /// Drop insertions, restore deletions
        #[arg(long, group = "settle")]
        reject: bool,
        /// Restrict to this author
        #[arg(long)]
        author: Option<String>,
        /// Output
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Overwrite input
        #[arg(long)]
        in_place: bool,
        /// JSON report
        #[arg(long)]
        json: bool,
    },
}

/// Run a track verb.
pub fn run(cmd: TrackCmd) -> Result<i32, String> {
    match cmd {
        TrackCmd::Apply {
            file,
            ops,
            output,
            in_place,
            author,
            date,
            granularity,
            lenient,
            dry_run,
            json,
            part,
        } => {
            let raw = read_json_arg(&ops)?;
            let ops: Vec<TrackOp> = serde_json::from_str(&raw)
                .map_err(|e| format!("usage: invalid --ops JSON: {e}"))?;
            if ops.is_empty() {
                return Err("usage: --ops is an empty array".into());
            }
            let gran = Granularity::parse(&granularity).map_err(|e| format!("usage: {e}"))?;
            let default_author = if ops.iter().any(|o| o.needs_author() && !o.has_own_author()) {
                Some(resolve_author(author.as_deref())?)
            } else {
                author.filter(|a| !a.trim().is_empty())
            };

            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let mut builder = doc.track(ops).granularity(gran);
            if let Some(a) = default_author {
                builder = builder.author(a);
            }
            if let Some(d) = date.filter(|s| !s.trim().is_empty()) {
                builder = builder.date(d);
            }
            if lenient {
                builder = builder.lenient();
            }
            if dry_run {
                builder = builder.dry_run();
            }
            if let Some(p) = part {
                builder = builder.part(p);
            }

            let outcome = builder.apply().map_err(|e| e.to_string())?;
            if json {
                print_json(&outcome.report)?;
            }
            if dry_run {
                return Ok(0);
            }
            let edited = outcome
                .document
                .ok_or_else(|| "track apply produced no document".to_string())?;
            let (path, _) = resolve_output(&file, output.as_deref(), in_place)?;
            atomic_write(&path, |tmp| edited.save(tmp).map_err(|e| e.to_string()))?;
            Ok(0)
        }
        TrackCmd::Redline {
            original,
            revised,
            output,
            author,
            granularity,
        } => {
            let author = resolve_author(author.as_deref())?;
            let gran = Granularity::parse(&granularity).map_err(|e| format!("usage: {e}"))?;
            let a = Docx::open(&original).map_err(|e| e.to_string())?;
            let b = Docx::open(&revised).map_err(|e| e.to_string())?;
            let out = track_redline(&a, &b, &author, gran).map_err(|e| e.to_string())?;
            atomic_write(&output, |tmp| out.save(tmp).map_err(|e| e.to_string()))?;
            Ok(0)
        }
        TrackCmd::Changes { file, author } => {
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let mut changes = doc.changes().map_err(|e| e.to_string())?;
            if let Some(want) = author {
                changes.retain(|c| c.author == want);
            }
            print_json(&changes)?;
            Ok(0)
        }
        TrackCmd::Comments { file, author } => {
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let mut comments = doc.comments().map_err(|e| e.to_string())?;
            if let Some(want) = author {
                comments.retain(|c| c.author == want);
            }
            print_json(&comments)?;
            Ok(0)
        }
        TrackCmd::Settle {
            file,
            accept,
            reject,
            author,
            output,
            in_place,
            json,
        } => {
            if accept == reject {
                return Err("usage: pass exactly one of --accept or --reject".into());
            }
            let op = if accept {
                TrackOp::AcceptAll { author }
            } else {
                TrackOp::RejectAll { author }
            };
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let outcome = doc.track([op]).apply().map_err(|e| e.to_string())?;
            if json {
                print_json(&outcome.report)?;
            }
            let edited = outcome
                .document
                .ok_or_else(|| "settle produced no document".to_string())?;
            let (path, _) = resolve_output(&file, output.as_deref(), in_place)?;
            atomic_write(&path, |tmp| edited.save(tmp).map_err(|e| e.to_string()))?;
            Ok(0)
        }
    }
}
