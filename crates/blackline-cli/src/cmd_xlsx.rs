//! `blackline xlsx …`

use std::path::PathBuf;

use clap::Subcommand;

use blackline_xlsx::{CreateSpec, EditOp, EditOptions, Xlsx};

use crate::util::{atomic_write, print_json, read_json_arg, resolve_output, slice_range};

/// XLSX verbs.
#[derive(Subcommand)]
pub enum XlsxCmd {
    /// Cell text view
    View {
        /// File
        file: PathBuf,
        /// Sheet name or index
        #[arg(long)]
        sheet: Option<String>,
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
pub fn run(cmd: XlsxCmd) -> Result<i32, String> {
    match cmd {
        XlsxCmd::View { file, sheet, json } => {
            let wb = Xlsx::open(&file).map_err(|e| e.to_string())?;
            let views = wb.view(sheet.as_deref()).map_err(|e| e.to_string())?;
            if json {
                print_json(&views)?;
            } else {
                for v in views {
                    println!("# {}", v.sheet);
                    for row in v.rows {
                        println!("{row}");
                    }
                }
            }
            Ok(0)
        }
        XlsxCmd::Info { file } => {
            let wb = Xlsx::open(&file).map_err(|e| e.to_string())?;
            print_json(&wb.info().map_err(|e| e.to_string())?)?;
            Ok(0)
        }
        XlsxCmd::Find {
            file,
            query,
            limit,
            json,
        } => {
            let wb = Xlsx::open(&file).map_err(|e| e.to_string())?;
            let hits = wb.find(&query, limit).map_err(|e| e.to_string())?;
            if json {
                print_json(&hits)?;
            } else {
                for h in hits {
                    println!("{}!{}\t{}", h.sheet, h.cell, h.text);
                }
            }
            Ok(0)
        }
        XlsxCmd::Edit {
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
            let mut wb = Xlsx::open(&file).map_err(|e| e.to_string())?;
            let opts = EditOptions { lenient, dry_run };
            let report = wb.edit(&ops, &opts).map_err(|e| e.to_string())?;
            if json {
                print_json(&report)?;
            } else {
                println!("{} applied, {} failed", report.applied, report.failed);
            }
            if !dry_run {
                let (out, inplace) = resolve_output(&file, output.as_deref(), in_place)?;
                if inplace {
                    atomic_write(&out, |tmp| wb.save(tmp).map_err(|e| e.to_string()))?;
                } else {
                    wb.save(&out).map_err(|e| e.to_string())?;
                }
            }
            Ok(if report.failed > 0 && !lenient { 1 } else { 0 })
        }
        XlsxCmd::Create { spec, output } => {
            let spec: CreateSpec = serde_json::from_str(&read_json_arg(&spec)?)
                .map_err(|e| format!("usage: invalid spec: {e}"))?;
            Xlsx::create(&spec)
                .map_err(|e| e.to_string())?
                .save(&output)
                .map_err(|e| e.to_string())?;
            println!("wrote {}", output.display());
            Ok(0)
        }
        XlsxCmd::Check { file } => {
            let wb = Xlsx::open(&file).map_err(|e| e.to_string())?;
            let r = wb.check().map_err(|e| e.to_string())?;
            print_json(&r)?;
            Ok(if r.passed() { 0 } else { 1 })
        }
        XlsxCmd::Cat { file, part } => {
            let wb = Xlsx::open(&file).map_err(|e| e.to_string())?;
            let part = part.unwrap_or_else(|| "xl/workbook.xml".into());
            let xml = wb.package().part_xml(&part).map_err(|e| e.to_string())?;
            print!("{}", String::from_utf8_lossy(&xml.to_pretty_bytes("  ")));
            Ok(0)
        }
        XlsxCmd::Parts { file, json } => {
            let wb = Xlsx::open(&file).map_err(|e| e.to_string())?;
            let names = wb.package().part_names();
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

#[allow(dead_code)]
fn _slice<T>(x: &[T]) -> &[T] {
    slice_range(x, None, None)
}
