//! `blackline pptx …`

use std::path::PathBuf;

use clap::Subcommand;

use blackline_pptx::{CreateSpec, EditOp, EditOptions, Pptx};

use crate::util::{atomic_write, print_json, read_json_arg, resolve_output};

/// PPTX verbs.
#[derive(Subcommand)]
pub enum PptxCmd {
    /// Slide text view
    View {
        /// File
        file: PathBuf,
        /// JSON
        #[arg(long)]
        json: bool,
    },
    /// Metrics (always JSON)
    Info {
        /// File
        file: PathBuf,
    },
    /// Search
    Find {
        /// File
        file: PathBuf,
        /// Query
        query: String,
        /// Limit
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// JSON
        #[arg(long)]
        json: bool,
    },
    /// Apply JSON edit ops
    Edit {
        /// File
        file: PathBuf,
        /// Ops
        #[arg(long)]
        ops: String,
        /// Output
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// In-place
        #[arg(long)]
        in_place: bool,
        /// Lenient
        #[arg(long)]
        lenient: bool,
        /// Dry run
        #[arg(long)]
        dry_run: bool,
        /// JSON
        #[arg(long)]
        json: bool,
    },
    /// Create
    Create {
        /// Spec
        #[arg(long)]
        spec: String,
        /// Output
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Health (always JSON)
    Check {
        /// File
        file: PathBuf,
    },
    /// Dump a part
    Cat {
        /// File
        file: PathBuf,
        /// Part
        part: Option<String>,
    },
    /// List parts
    Parts {
        /// File
        file: PathBuf,
        /// JSON
        #[arg(long)]
        json: bool,
    },
}

/// Run.
pub fn run(cmd: PptxCmd) -> Result<i32, String> {
    match cmd {
        PptxCmd::View { file, json } => {
            let p = Pptx::open(&file).map_err(|e| e.to_string())?;
            let views = p.view().map_err(|e| e.to_string())?;
            if json {
                print_json(&views)?;
            } else {
                for s in views {
                    println!("# slide {}", s.index);
                    for (i, t) in s.elements.iter().enumerate() {
                        println!("  [{}] {t}", i + 1);
                    }
                }
            }
            Ok(0)
        }
        PptxCmd::Info { file } => {
            let p = Pptx::open(&file).map_err(|e| e.to_string())?;
            print_json(&p.info().map_err(|e| e.to_string())?)?;
            Ok(0)
        }
        PptxCmd::Find {
            file,
            query,
            limit,
            json,
        } => {
            let p = Pptx::open(&file).map_err(|e| e.to_string())?;
            let hits = p.find(&query, limit).map_err(|e| e.to_string())?;
            if json {
                print_json(&hits)?;
            } else {
                for h in hits {
                    println!("slide {} element {}: {}", h.slide, h.element, h.text);
                }
            }
            Ok(0)
        }
        PptxCmd::Edit {
            file,
            ops,
            output,
            in_place,
            lenient,
            dry_run,
            json,
        } => {
            let ops: Vec<EditOp> = serde_json::from_str(&read_json_arg(&ops)?)
                .map_err(|e| format!("usage: invalid ops JSON: {e}"))?;
            let mut p = Pptx::open(&file).map_err(|e| e.to_string())?;
            let report = p
                .edit(&ops, &EditOptions { lenient, dry_run })
                .map_err(|e| e.to_string())?;
            if json {
                print_json(&report)?;
            } else {
                println!("{} applied, {} failed", report.applied, report.failed);
            }
            if !dry_run {
                let (out, inplace) = resolve_output(&file, output.as_deref(), in_place)?;
                if inplace {
                    atomic_write(&out, |tmp| p.save(tmp).map_err(|e| e.to_string()))?;
                } else {
                    p.save(&out).map_err(|e| e.to_string())?;
                }
            }
            Ok(if report.failed > 0 && !lenient { 1 } else { 0 })
        }
        PptxCmd::Create { spec, output } => {
            let spec: CreateSpec = serde_json::from_str(&read_json_arg(&spec)?)
                .map_err(|e| format!("usage: invalid spec: {e}"))?;
            Pptx::create(&spec)
                .map_err(|e| e.to_string())?
                .save(&output)
                .map_err(|e| e.to_string())?;
            println!("wrote {}", output.display());
            Ok(0)
        }
        PptxCmd::Check { file } => {
            let p = Pptx::open(&file).map_err(|e| e.to_string())?;
            let r = p.check().map_err(|e| e.to_string())?;
            print_json(&r)?;
            Ok(if r.passed() { 0 } else { 1 })
        }
        PptxCmd::Cat { file, part } => {
            let p = Pptx::open(&file).map_err(|e| e.to_string())?;
            let part = part.unwrap_or_else(|| "ppt/presentation.xml".into());
            let xml = p.package().part_xml(&part).map_err(|e| e.to_string())?;
            print!("{}", String::from_utf8_lossy(&xml.to_pretty_bytes("  ")));
            Ok(0)
        }
        PptxCmd::Parts { file, json } => {
            let p = Pptx::open(&file).map_err(|e| e.to_string())?;
            let names = p.package().part_names();
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
