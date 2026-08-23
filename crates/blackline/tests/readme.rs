//! The READMEs are a product surface: the "use it from a coding agent" block
//! is meant to be pasted into an agent that will run it verbatim. A stale flag
//! there is worse than a typo, because the agent cannot tell it is stale.
//!
//! So rather than trusting the prose, extract every `bl …` invocation from the
//! READMEs and check it against the CLI's own help: the subcommand chain must
//! exist, and every long flag must appear in that subcommand's `--help`. Edit
//! the README and this picks the change up automatically -- there is no second
//! list here to drift out of step.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bl() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin("bl"))
}

/// READMEs to scan. The crate README ships inside the package; the workspace
/// one does not, so a published-crate test run simply skips it.
fn readmes() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    vec![root.join("README.md"), root.join("../../README.md")]
        .into_iter()
        .filter(|p| p.is_file())
        .collect()
}

/// One `bl …` invocation lifted out of the prose.
#[derive(Debug)]
struct Invocation {
    file: String,
    line: usize,
    subcommands: Vec<String>,
    flags: Vec<String>,
}

/// Join `\`-continued lines so a wrapped command is parsed as one, keeping
/// only lines inside fenced code blocks -- prose that happens to open with the
/// word "blackline" is not a command.
fn logical_lines(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    let mut in_fence = false;
    for (i, raw) in text.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            pending = None; // a fence boundary ends any continuation
            continue;
        }
        if !in_fence {
            continue;
        }
        let (start, mut acc) = pending.take().unwrap_or((i + 1, String::new()));
        if !acc.is_empty() {
            acc.push(' ');
        }
        let continues = trimmed.ends_with('\\');
        acc.push_str(trimmed.trim_end_matches('\\').trim_end());
        if continues {
            pending = Some((start, acc));
        } else {
            out.push((start, acc));
        }
    }
    if let Some(rest) = pending {
        out.push(rest);
    }
    out
}

/// Split on whitespace but keep quoted arguments together, so
/// `--sheet "Cap Table"` is one token and its spaces are not read as flags.
fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c.is_whitespace() => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

/// A leading bare word is a subcommand; anything with a path separator, dot, or
/// leading dash ends the chain.
fn looks_like_subcommand(tok: &str) -> bool {
    !tok.is_empty()
        && !tok.starts_with('-')
        && tok
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

fn parse(path: &Path) -> Vec<Invocation> {
    let text = std::fs::read_to_string(path).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let label = if path.to_string_lossy().contains("../..") {
        format!("workspace {name}")
    } else {
        format!("crate {name}")
    };

    let mut found = Vec::new();
    for (line, logical) in logical_lines(&text) {
        let tokens = tokenize(&logical);
        let Some(first) = tokens.first() else {
            continue;
        };
        if first != "bl" && first != "blackline" {
            continue;
        }
        let rest = &tokens[1..];
        let subcommands: Vec<String> = rest
            .iter()
            .take_while(|t| looks_like_subcommand(t))
            .cloned()
            .collect();
        if subcommands.is_empty() {
            continue; // e.g. `bl --version`
        }
        let flags: Vec<String> = rest
            .iter()
            .filter(|t| t.starts_with("--") && t.len() > 2)
            .cloned()
            .collect();
        found.push(Invocation {
            file: label.clone(),
            line,
            subcommands,
            flags,
        });
    }
    found
}

#[test]
fn readme_commands_exist_in_the_cli() {
    let files = readmes();
    assert!(!files.is_empty(), "no README found to check");

    let invocations: Vec<Invocation> = files.iter().flat_map(|p| parse(p)).collect();
    assert!(
        invocations.len() >= 10,
        "only found {} bl invocations — the parser is probably broken, \
         not the READMEs",
        invocations.len()
    );

    let mut problems = Vec::new();

    for inv in &invocations {
        let out = bl()
            .args(&inv.subcommands)
            .arg("--help")
            .output()
            .expect("failed to run bl");

        let where_ = format!(
            "{}:{} `bl {}`",
            inv.file,
            inv.line,
            inv.subcommands.join(" ")
        );

        if !out.status.success() {
            problems.push(format!("{where_}: subcommand does not exist"));
            continue;
        }

        let help = String::from_utf8_lossy(&out.stdout);
        for flag in &inv.flags {
            if !help.contains(flag.as_str()) {
                problems.push(format!(
                    "{where_}: documents {flag}, which is not in --help"
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "the READMEs document {} thing(s) the CLI does not have:\n  {}\n\n\
         Either the docs are stale or the CLI changed. Fix whichever is wrong.",
        problems.len(),
        problems.join("\n  ")
    );

    eprintln!(
        "checked {} documented invocations across {} README(s)",
        invocations.len(),
        files.len()
    );
}
