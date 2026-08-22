//! Cross-format XML / package verbs.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use std::collections::BTreeMap;

use blackline_core::formula::{self, Value};
use blackline_core::package::Package;
use blackline_core::patch::{self, PatchOp};
use blackline_core::tree::{self, NodePath, TreeOp, TreeOpReport};
use blackline_core::update::{self, UpdateOp};
use blackline_core::{opc, validate};

use crate::util::{print_json, read_json_arg, resolve_output};

/// Package-level and raw-XML verbs.
#[derive(Subcommand)]
pub enum XmlCmd {
    /// Read a node (or the whole part) as XML
    Get {
        /// Package
        file: PathBuf,
        /// Part name
        part: String,
        /// Node path (`body/p[0]/r[0]/t`)
        #[arg(long)]
        path: Option<String>,
    },
    /// Apply TreeOp mutations to a package
    Edit {
        /// Package
        file: PathBuf,
        /// JSON array of tree ops. Each op may include `part`.
        #[arg(long)]
        ops: String,
        /// Output
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Strict (default)
        #[arg(long)]
        lenient: bool,
        /// JSON report
        #[arg(long)]
        json: bool,
        /// Validate without writing
        #[arg(long)]
        dry_run: bool,
        /// Overwrite the input file
        #[arg(long)]
        in_place: bool,
    },
    /// Apply RFC 5261 add/replace/remove (compiles to TreeOp)
    Patch {
        /// Package
        file: PathBuf,
        /// JSON array or RFC 5261 `<diff>` document
        #[arg(long)]
        ops: String,
        /// Part name (required unless each JSON op has `part`)
        #[arg(long)]
        part: Option<String>,
        /// Output
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Overwrite the input file
        #[arg(long)]
        in_place: bool,
        /// Compile and apply in memory; print TreeOps, write nothing
        #[arg(long)]
        dry_run: bool,
        /// Strict (default)
        #[arg(long)]
        lenient: bool,
        /// JSON report
        #[arg(long)]
        json: bool,
    },
    /// Apply XQuery Update verbs (compiles to TreeOp)
    Update {
        /// Package
        file: PathBuf,
        /// JSON array or `delete node //ins; insert node <x/> into /body`
        #[arg(long)]
        ops: String,
        /// Part name (required unless each JSON op has `part`)
        #[arg(long)]
        part: Option<String>,
        /// Output
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Overwrite the input file
        #[arg(long)]
        in_place: bool,
        /// Compile and apply in memory; print TreeOps, write nothing
        #[arg(long)]
        dry_run: bool,
        /// Strict (default)
        #[arg(long)]
        lenient: bool,
        /// JSON report
        #[arg(long)]
        json: bool,
    },
    /// Evaluate an XML formula against a part (`count(//ins)`, `text(//p[0])`)
    Eval {
        /// Package
        file: PathBuf,
        /// Part name
        part: String,
        /// Formula
        formula: String,
    },
    /// Select nodes with an RFC 5261 / XPath-subset selector (`//ins`, `/body/p[0]`)
    Select {
        /// Package
        file: PathBuf,
        /// Part name
        part: String,
        /// Selector (or a formula that yields a node-set)
        selector: String,
    },
}

/// Unpack a package into pretty-printed XML parts.
pub fn unpack(input: &Path, output: &Path) -> Result<i32, String> {
    let n = opc::unpack_archive(input, output).map_err(|e| e.to_string())?;
    println!("unpacked {n} XML part(s) into {}", output.display());
    Ok(0)
}

/// Pack a directory of parts back into an OOXML package.
pub fn pack(input: &Path, output: &Path) -> Result<i32, String> {
    opc::pack_dir(input, output).map_err(|e| e.to_string())?;
    let pkg = Package::open(output).map_err(|e| e.to_string())?;
    let report = validate::check(&pkg).map_err(|e| e.to_string())?;
    if !report.passed() {
        eprintln!("warning: packed file failed health check");
        print_json(&report)?;
        return Ok(1);
    }
    println!("wrote {}", output.display());
    Ok(0)
}

/// Run xml get/edit/eval/select/patch/update.
pub fn run(cmd: XmlCmd) -> Result<i32, String> {
    match cmd {
        XmlCmd::Get { file, part, path } => {
            let pkg = Package::open(&file).map_err(|e| e.to_string())?;
            let doc = pkg.part_xml(&part).map_err(|e| e.to_string())?;
            if let Some(p) = path {
                let node = NodePath::parse(&p)
                    .map_err(|e| e.to_string())?
                    .resolve(&doc.root)
                    .map_err(|e| e.to_string())?;
                println!("{}", node.to_xml_string());
            } else {
                print!("{}", String::from_utf8_lossy(&doc.to_pretty_bytes("  ")));
            }
            Ok(0)
        }
        XmlCmd::Edit {
            file,
            ops,
            output,
            lenient,
            json,
            dry_run,
            in_place,
        } => {
            #[derive(serde::Deserialize)]
            struct PartOp {
                part: String,
                #[serde(flatten)]
                op: TreeOp,
            }
            let raw = read_json_arg(&ops)?;
            let ops: Vec<PartOp> =
                serde_json::from_str(&raw).map_err(|e| format!("usage: invalid tree ops: {e}"))?;
            let mut pkg = Package::open(&file).map_err(|e| e.to_string())?;
            // Group by part.
            use std::collections::BTreeMap;
            let mut by_part: BTreeMap<String, Vec<TreeOp>> = BTreeMap::new();
            for o in ops {
                by_part.entry(o.part).or_default().push(o.op);
            }
            let mut all_reports = Vec::new();
            for (part, ops) in by_part {
                let mut doc = pkg.part_xml(&part).map_err(|e| e.to_string())?;
                let reports =
                    tree::apply_ops(&mut doc.root, &ops, !lenient).map_err(|e| e.to_string())?;
                pkg.set_part_xml(&part, &doc);
                all_reports.extend(reports);
            }
            finish_write(
                &file,
                pkg,
                &all_reports,
                output.as_deref(),
                in_place,
                dry_run,
                json,
            )
        }
        XmlCmd::Patch {
            file,
            ops,
            part,
            output,
            in_place,
            dry_run,
            lenient,
            json,
        } => {
            let raw = read_json_arg(&ops)?;
            let grouped = group_patch_ops(&raw, part.as_deref())?;
            run_compiled(
                &file,
                grouped,
                output.as_deref(),
                in_place,
                dry_run,
                json,
                !lenient,
            )
        }
        XmlCmd::Update {
            file,
            ops,
            part,
            output,
            in_place,
            dry_run,
            lenient,
            json,
        } => {
            let raw = read_json_arg(&ops)?;
            let grouped = group_update_ops(&raw, part.as_deref())?;
            run_compiled(
                &file,
                grouped,
                output.as_deref(),
                in_place,
                dry_run,
                json,
                !lenient,
            )
        }
        XmlCmd::Eval {
            file,
            part,
            formula,
        } => {
            let pkg = Package::open(&file).map_err(|e| e.to_string())?;
            let doc = pkg.part_xml(&part).map_err(|e| e.to_string())?;
            let value = formula::eval_str(&doc.root, &formula).map_err(formula_err)?;
            print_json(&value)?;
            Ok(0)
        }
        XmlCmd::Select {
            file,
            part,
            selector,
        } => {
            let pkg = Package::open(&file).map_err(|e| e.to_string())?;
            let doc = pkg.part_xml(&part).map_err(|e| e.to_string())?;
            let value = formula::eval_str(&doc.root, &selector).map_err(formula_err)?;
            match value {
                Value::Nodes(matches) => {
                    print_json(&serde_json::json!({
                        "count": matches.len(),
                        "matches": matches,
                    }))?;
                    Ok(0)
                }
                other => Err(format!(
                    "usage: xml select expects a selector (got {}). Use xml eval for count/text/contains.",
                    other_kind(&other)
                )),
            }
        }
    }
}

fn formula_err(e: blackline_core::CoreError) -> String {
    match e {
        blackline_core::CoreError::Formula(msg)
        | blackline_core::CoreError::Patch(msg)
        | blackline_core::CoreError::Update(msg) => format!("usage: {msg}"),
        other => other.to_string(),
    }
}

enum Compiled {
    Patch(Vec<PatchOp>),
    Update(Vec<UpdateOp>),
}

fn group_patch_ops(raw: &str, part: Option<&str>) -> Result<BTreeMap<String, Compiled>, String> {
    #[derive(serde::Deserialize)]
    struct Wrap {
        part: Option<String>,
        #[serde(flatten)]
        op: PatchOp,
    }
    let trimmed = raw.trim();
    if trimmed.starts_with('<') || part.is_some() {
        let ops = patch::parse_ops(trimmed).map_err(formula_err)?;
        let p = part.map(str::to_string).ok_or_else(|| {
            "usage: xml patch needs --part, or a JSON array with per-op part".to_string()
        })?;
        return Ok(BTreeMap::from([(p, Compiled::Patch(ops))]));
    }
    let wraps: Vec<Wrap> =
        serde_json::from_str(trimmed).map_err(|e| format!("usage: invalid patch JSON: {e}"))?;
    let mut by_part: BTreeMap<String, Vec<PatchOp>> = BTreeMap::new();
    for w in wraps {
        let p = w
            .part
            .ok_or_else(|| "usage: each patch op needs part, or pass --part".to_string())?;
        by_part.entry(p).or_default().push(w.op);
    }
    Ok(by_part
        .into_iter()
        .map(|(p, ops)| (p, Compiled::Patch(ops)))
        .collect())
}

fn group_update_ops(raw: &str, part: Option<&str>) -> Result<BTreeMap<String, Compiled>, String> {
    #[derive(serde::Deserialize)]
    struct Wrap {
        part: Option<String>,
        #[serde(flatten)]
        op: UpdateOp,
    }
    let trimmed = raw.trim();
    if !trimmed.starts_with('[') {
        let ops = update::parse_ops(trimmed).map_err(formula_err)?;
        let p = part
            .map(str::to_string)
            .ok_or_else(|| "usage: xml update needs --part for a text expression".to_string())?;
        return Ok(BTreeMap::from([(p, Compiled::Update(ops))]));
    }
    // JSON: either UpdateOp[] with --part, or [{part, action, ...}].
    if let Ok(ops) = serde_json::from_str::<Vec<UpdateOp>>(trimmed) {
        let p = part.map(str::to_string).ok_or_else(|| {
            "usage: xml update needs --part, or a JSON array with per-op part".to_string()
        })?;
        return Ok(BTreeMap::from([(p, Compiled::Update(ops))]));
    }
    let wraps: Vec<Wrap> =
        serde_json::from_str(trimmed).map_err(|e| format!("usage: invalid update JSON: {e}"))?;
    let mut by_part: BTreeMap<String, Vec<UpdateOp>> = BTreeMap::new();
    for w in wraps {
        let p = w
            .part
            .or_else(|| part.map(str::to_string))
            .ok_or_else(|| "usage: each update op needs part, or pass --part".to_string())?;
        by_part.entry(p).or_default().push(w.op);
    }
    Ok(by_part
        .into_iter()
        .map(|(p, ops)| (p, Compiled::Update(ops)))
        .collect())
}

fn run_compiled(
    file: &Path,
    grouped: BTreeMap<String, Compiled>,
    output: Option<&Path>,
    in_place: bool,
    dry_run: bool,
    json: bool,
    strict: bool,
) -> Result<i32, String> {
    let mut pkg = Package::open(file).map_err(|e| e.to_string())?;
    let mut planned: Vec<TreeOp> = Vec::new();
    let mut reports: Vec<TreeOpReport> = Vec::new();
    for (part, compiled) in grouped {
        let mut doc = pkg.part_xml(&part).map_err(|e| e.to_string())?;
        let ops = match compiled {
            Compiled::Patch(ops) => patch::plan(&doc.root, &ops).map_err(formula_err)?,
            Compiled::Update(ops) => update::plan(&doc.root, &ops).map_err(formula_err)?,
        };
        planned.extend(ops.clone());
        let part_reports =
            tree::apply_ops(&mut doc.root, &ops, strict).map_err(|e| e.to_string())?;
        reports.extend(part_reports);
        pkg.set_part_xml(&part, &doc);
    }
    if dry_run {
        if json {
            print_json(&serde_json::json!({
                "dry_run": true,
                "compiled": planned,
                "reports": reports,
            }))?;
        } else {
            println!("dry-run: {} TreeOp(s)", planned.len());
            for op in &planned {
                println!("  {}", describe_op(op));
            }
        }
        return Ok(0);
    }
    finish_write(file, pkg, &reports, output, in_place, false, json)
}

fn finish_write(
    input: &Path,
    pkg: Package,
    reports: &[TreeOpReport],
    output: Option<&Path>,
    in_place: bool,
    dry_run: bool,
    json: bool,
) -> Result<i32, String> {
    if dry_run {
        if json {
            print_json(&reports)?;
        } else {
            for r in reports {
                println!("[{}] {} {} — {}", r.index, r.action, r.status, r.detail);
            }
        }
        return Ok(0);
    }
    let (out, _) = resolve_output(input, output, in_place)?;
    pkg.save(&out).map_err(|e| e.to_string())?;
    if json {
        print_json(&reports)?;
    } else {
        for r in reports {
            println!("[{}] {} {} — {}", r.index, r.action, r.status, r.detail);
        }
    }
    Ok(0)
}

fn describe_op(op: &TreeOp) -> String {
    match op {
        TreeOp::SetAttr { path, name, value } => format!("set_attr {path} {name}={value}"),
        TreeOp::RemoveAttr { path, name } => format!("remove_attr {path} {name}"),
        TreeOp::SetText { path, text } => format!("set_text {path} {:?}", truncate(text)),
        TreeOp::InsertChild { path, index, xml } => {
            format!(
                "insert_child {path}[{}] {}",
                index.map(|i| i.to_string()).unwrap_or_else(|| "end".into()),
                truncate(xml)
            )
        }
        TreeOp::RemoveChild { path, index } => format!("remove_child {path}[{index}]"),
        TreeOp::Replace { path, xml } => format!("replace {path} {}", truncate(xml)),
        TreeOp::Rename { path, name } => format!("rename {path} {name}"),
    }
}

fn truncate(s: &str) -> String {
    const MAX: usize = 48;
    let count = s.chars().count();
    if count <= MAX {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(MAX).collect::<String>())
    }
}

fn other_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Nodes(_) => "nodes",
    }
}
