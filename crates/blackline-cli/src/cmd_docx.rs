//! `blackline docx …`

use std::path::PathBuf;

use clap::Subcommand;

use blackline_docx::{redline, CreateSpec, Docx, EditOp, Granularity, SearchQuery, DEFAULT_LIMIT};

use crate::util::{
    atomic_write, print_json, read_json_arg, resolve_author, resolve_output, slice_range,
};

/// DOCX verbs.
#[derive(Subcommand)]
pub enum DocxCmd {
    /// Numbered text view
    View {
        /// DOCX file
        file: PathBuf,
        /// First view index (1-based)
        #[arg(long)]
        from: Option<usize>,
        /// Last view index
        #[arg(long)]
        to: Option<usize>,
        /// Show tracked-change markup
        #[arg(long)]
        raw: bool,
        /// JSON
        #[arg(long)]
        json: bool,
    },
    /// Heading outline
    Outline {
        /// File
        file: PathBuf,
        /// JSON
        #[arg(long)]
        json: bool,
    },
    /// Search
    Find {
        /// File
        file: PathBuf,
        /// Query
        query: String,
        /// Case-sensitive
        #[arg(long)]
        case_sensitive: bool,
        /// Whole word
        #[arg(long)]
        whole_word: bool,
        /// Limit
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,
        /// From index
        #[arg(long)]
        from: Option<usize>,
        /// To index
        #[arg(long)]
        to: Option<usize>,
        /// JSON
        #[arg(long)]
        json: bool,
    },
    /// Metrics (always JSON)
    Info {
        /// Files
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Apply JSON edit ops
    Edit {
        /// File
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
        /// Tracked changes
        #[arg(long)]
        track: bool,
        /// Author
        #[arg(long)]
        author: Option<String>,
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
    /// Create from a JSON spec
    Create {
        /// Spec JSON, @file, or -
        #[arg(long)]
        spec: String,
        /// Output
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Package health (always JSON)
    Check {
        /// File
        file: PathBuf,
        /// Original, for redline integrity
        #[arg(long)]
        original: Option<PathBuf>,
    },
    /// List tracked changes (always JSON)
    Changes {
        /// File
        file: PathBuf,
    },
    /// List comments (always JSON)
    Comments {
        /// File
        file: PathBuf,
    },
    /// Compare two documents into a redline
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
    /// Dump a package part as XML
    Cat {
        /// File
        file: PathBuf,
        /// Part name (default: word/document.xml)
        part: Option<String>,
    },
    /// List package parts
    Parts {
        /// File
        file: PathBuf,
        /// JSON
        #[arg(long)]
        json: bool,
    },
}

/// Run a docx verb.
pub fn run(cmd: DocxCmd) -> Result<i32, String> {
    match cmd {
        DocxCmd::View {
            file,
            from,
            to,
            raw,
            json,
        } => {
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let lines = doc.view(raw).map_err(|e| e.to_string())?;
            let slice = slice_range(&lines, from, to);
            if json {
                print_json(&slice)?;
            } else {
                for l in slice {
                    match l.heading {
                        Some(h) => println!("{}| H{h}: {}", l.index, l.text),
                        None => println!("{}| {}", l.index, l.text),
                    }
                }
            }
            Ok(0)
        }
        DocxCmd::Outline { file, json } => {
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let outline = doc.outline().map_err(|e| e.to_string())?;
            if json {
                print_json(&outline)?;
            } else {
                for e in outline {
                    println!("{:>4}  H{}  {}", e.index, e.level, e.text);
                }
            }
            Ok(0)
        }
        DocxCmd::Find {
            file,
            query,
            case_sensitive,
            whole_word,
            limit,
            from,
            to,
            json,
        } => {
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let mut q = SearchQuery::new(query).limit(limit);
            q.case_sensitive = case_sensitive;
            q.whole_word = whole_word;
            q.from = from;
            q.to = to;
            let hits = doc.search(&q).map_err(|e| e.to_string())?;
            if json {
                print_json(&hits)?;
            } else {
                println!("{} match(es)", hits.total);
                for h in &hits.matches {
                    println!("[{}] {}", h.index, h.text);
                }
            }
            Ok(0)
        }
        DocxCmd::Info { files } => {
            let mut all = Vec::new();
            for f in files {
                let doc = Docx::open(&f).map_err(|e| e.to_string())?;
                all.push(doc.info().map_err(|e| e.to_string())?);
            }
            if all.len() == 1 {
                print_json(&all[0])?;
            } else {
                print_json(&all)?;
            }
            Ok(0)
        }
        DocxCmd::Edit {
            file,
            ops,
            output,
            in_place,
            track,
            author,
            granularity,
            lenient,
            dry_run,
            json,
            part,
        } => {
            let ops_json = read_json_arg(&ops)?;
            let ops: Vec<EditOp> = serde_json::from_str(&ops_json)
                .map_err(|e| format!("usage: invalid ops JSON: {e}"))?;
            let gran = Granularity::parse(&granularity).map_err(|e| format!("usage: {e}"))?;
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let mut builder = doc.edit(ops).granularity(gran);
            if let Some(p) = part {
                builder = builder.part(p);
            }
            if track {
                builder = builder.tracked(resolve_author(author.as_deref())?);
            } else if let Some(a) = author {
                builder = builder.author(a);
            } else if std::env::var("BLACKLINE_AUTHOR").is_ok() {
                if let Ok(a) = resolve_author(None) {
                    builder = builder.author(a);
                }
            }
            if lenient {
                builder = builder.lenient();
            }
            if dry_run {
                builder = builder.dry_run();
            }
            let outcome = builder.apply().map_err(|e| e.to_string())?;
            if json {
                print_json(&outcome.report)?;
            } else {
                println!(
                    "{} applied, {} failed ({})",
                    outcome.report.applied, outcome.report.failed, outcome.report.mode
                );
                for op in &outcome.report.ops {
                    println!("  [{}] {} {} — {}", op.index, op.op, op.status, op.detail);
                }
            }
            if let Some(edited) = outcome.document {
                if !dry_run {
                    let (out, inplace) = resolve_output(&file, output.as_deref(), in_place)?;
                    if inplace {
                        atomic_write(&out, |tmp| edited.save(tmp).map_err(|e| e.to_string()))?;
                    } else {
                        edited.save(&out).map_err(|e| e.to_string())?;
                    }
                }
            }
            Ok(if outcome.report.failed > 0 && !lenient {
                1
            } else {
                0
            })
        }
        DocxCmd::Create { spec, output } => {
            let spec: CreateSpec = serde_json::from_str(&read_json_arg(&spec)?)
                .map_err(|e| format!("usage: invalid spec JSON: {e}"))?;
            let doc = Docx::create(&spec).map_err(|e| e.to_string())?;
            doc.save(&output).map_err(|e| e.to_string())?;
            println!("wrote {}", output.display());
            Ok(0)
        }
        DocxCmd::Check { file, original } => {
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let report = if let Some(orig) = original {
                let o = Docx::open(&orig).map_err(|e| e.to_string())?;
                doc.check_against(&o).map_err(|e| e.to_string())?
            } else {
                doc.check().map_err(|e| e.to_string())?
            };
            print_json(&report)?;
            Ok(if report.passed() { 0 } else { 1 })
        }
        DocxCmd::Changes { file } => {
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            print_json(&doc.changes().map_err(|e| e.to_string())?)?;
            Ok(0)
        }
        DocxCmd::Comments { file } => {
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            print_json(&doc.comments().map_err(|e| e.to_string())?)?;
            Ok(0)
        }
        DocxCmd::Redline {
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
            let out = redline(&a, &b, &author, gran).map_err(|e| e.to_string())?;
            out.save(&output).map_err(|e| e.to_string())?;
            println!("wrote {}", output.display());
            Ok(0)
        }
        DocxCmd::Cat { file, part } => {
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let part = part.unwrap_or_else(|| "word/document.xml".into());
            let xml = doc.package().part_xml(&part).map_err(|e| e.to_string())?;
            print!("{}", String::from_utf8_lossy(&xml.to_pretty_bytes("  ")));
            Ok(0)
        }
        DocxCmd::Parts { file, json } => {
            let doc = Docx::open(&file).map_err(|e| e.to_string())?;
            let names: Vec<&str> = doc.package().part_names();
            if json {
                print_json(&names)?;
            } else {
                for n in names {
                    println!("{n}");
                }
            }
            Ok(0)
        }
    }
}
