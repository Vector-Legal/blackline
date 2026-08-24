<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.png">
    <img src="assets/logo.png" alt="blackline" width="140">
  </picture>
</p>

# blackline

[![CI](https://github.com/Vector-Legal/blackline/actions/workflows/ci.yml/badge.svg)](https://github.com/Vector-Legal/blackline/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/blackline?logo=rust&label=crates.io)](https://crates.io/crates/blackline)
[![docs.rs](https://img.shields.io/docsrs/blackline-core?logo=docsdotrs&label=docs.rs)](https://docs.rs/blackline-core)
[![MSRV](https://img.shields.io/badge/msrv-1.88+-blue.svg)](rust-toolchain.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Unopinionated Office Open XML toolkit. The library edits the XML that
actually lives in a `.docx` / `.xlsx` / `.pptx` package. It does not convert
documents to HTML, Markdown, or PDF.

**This repository is AI-generated.** The code was written with AI
coding agents (Cursor). Read it and test it yourself before you depend
on it.

`blackline` is a **CLI** (`blackline` / `bl`) and a set of **Rust crates**:

| Crate | Purpose |
|-------|---------|
| [`blackline-core`](crates/blackline-core) | OPC package, XML DOM, `TreeOp`, formulas, RFC 5261 patch, XQuery Update, LCS diff |
| [`blackline-docx`](crates/blackline-docx) | WordprocessingML: view, search, edit, redline, track, comments, create, check |
| [`blackline-xlsx`](crates/blackline-xlsx) | SpreadsheetML: sheets, cells, shared strings, create, edit, check |
| [`blackline-pptx`](crates/blackline-pptx) | PresentationML: slides, text frames, create, edit, check |
| [`blackline-ai`](crates/blackline-ai) | Local AI frontend: a prompt becomes blackline ops |
| [`blackline`](crates/blackline) | Agent-first noun-verb CLI (`blackline` / `bl`) |

Docs: [architecture](docs/architecture.md) · [CLI reference](docs/cli.md) ·
[ai](docs/ai.md) ·
[releasing](docs/releasing.md) · [going public](docs/going-public.md) ·
[changelog](CHANGELOG.md)

```bash
cargo install blackline              # binaries: blackline, bl
cargo install blackline --features kalosm   # same CLI, plus a working `bl ai`
cargo add blackline-docx            # or blackline-xlsx / blackline-pptx
```

## Use it from a coding agent

blackline is built to be driven by an agent: every verb is noun-first, the
inspection commands emit JSON, and exit codes distinguish a failed operation
(1) from bad usage (2). Paste the block below into Claude Code, Codex, Cursor,
or any agent with shell access and it has what it needs.

<details>
<summary><b>Copy this into your agent</b></summary>

````text
Install and use `blackline`, a CLI for editing DOCX / XLSX / PPTX by
operating on the OOXML inside the package (no HTML/Markdown/PDF conversion).

Install (Rust toolchain required; `bl` is an alias for `blackline`):

    cargo install blackline
    bl --version

Grammar is `bl <format> <verb> FILE [args]`, where format is docx | xlsx |
pptx, plus the `track`, `xml`, `ai`, `unpack`, `pack` and `fixtures` commands.

Rules that matter:
- `info`, `check`, `changes`, `comments` and `--json` always emit JSON. Parse
  that rather than the human-readable `view` output.
- Indices in edit and track ops are **1-based**.
- Any tracked change or comment needs `--author NAME`, or set BLACKLINE_AUTHOR.
- Edits are strict: if one op fails, nothing is written. Add `--dry-run` to
  test a recipe, `--lenient` for best-effort.
- Exit codes: 0 success, 1 operation failed, 2 usage error.
- Write with `-o OUT` or `--in-place`. JSON args accept inline JSON,
  `@file.json`, or `-` for stdin.

Read a document:

    bl docx view contract.docx --from 1 --to 40
    bl docx outline contract.docx
    bl docx info contract.docx                  # JSON
    bl docx find contract.docx "Purchase Price" --whole-word --json

Edit, leaving Word-native tracked changes a lawyer can accept or reject:

    bl track apply contract.docx \
      --ops '[{"op":"replace","index":1,"old":"thirty days","new":"sixty days"}]' \
      -o revised.docx --author "Jane Doe"

    bl docx changes revised.docx                # JSON: who changed what
    bl track settle revised.docx --accept -o final.docx
    bl track settle revised.docx --reject -o original.docx

Diff two documents into a redline:

    bl track redline original.docx revised.docx -o redline.docx --author "Jane Doe"

Always verify after mutating, and compare against the original:

    bl docx check revised.docx --original contract.docx

Spreadsheets and decks work the same way:

    bl xlsx info model.xlsx                     # sheet names are in `sheet_names`
    bl xlsx view model.xlsx --sheet "Cap Table"
    bl pptx edit deck.pptx --ops '[{"op":"set_text","slide":1,"element":1,"text":"Q3"}]' -o out.pptx

Need something the verbs do not cover? Drop to the XML:

    bl xml eval file.docx word/document.xml 'count(//ins)'
    bl xml select file.docx word/document.xml '//p[0]'

`bl fixtures ./corpus` writes sample DOCX/XLSX/PPTX files to experiment on
without needing real documents. Run `bl <format> --help` for the full verb
list, or see docs/cli.md.

Natural language (local AI on the same CLI; install with `--features kalosm`,
and add `,metal` on Apple Silicon or `,cuda` on NVIDIA):

    bl ai contract.docx "change thirty days to sixty days" \
        -o revised.docx --author "Jane Doe"
````

</details>

## CLI

```
blackline <format> <verb>
```

The binary also installs as **`bl`**.

```
docx   view | outline | info | find | edit | create | check |
       changes | comments | redline | cat | parts
xlsx   view | info | find | edit | create | check | cat | parts
pptx   view | info | find | edit | create | check | cat | parts
xml    get | eval | select | edit | patch | update
track  apply | redline | changes | comments | settle
ai     FILE INSTRUCTION
unpack FILE DIR
pack   DIR FILE
fixtures DIR
```

```bash
# Read
bl docx view contract.docx --from 1 --to 20
bl docx outline contract.docx
bl docx info contract.docx                 # always JSON
bl docx find contract.docx "Purchase Price" --whole-word --json

# Edit — strict by default: any failed op aborts, nothing is written
bl docx edit contract.docx --ops @ops.json -o out.docx
bl docx edit contract.docx --ops @ops.json --dry-run --json

# Tracked changes — author is required (or BLACKLINE_AUTHOR)
bl docx edit contract.docx --ops @ops.json -o out.docx \
    --track --author "Jane Doe" --granularity word

# Track module — multi-author, surgical, minimized redline
bl track apply contract.docx --ops @ops.json -o out.docx --author "Jane Doe"
bl track redline original.docx revised.docx -o redline.docx --author "Jane Doe"
bl track changes redline.docx --author "Jane Doe"
bl track settle redline.docx --accept --author "Bob" -o accepted.docx

# Compare two documents into a redline
bl docx redline original.docx revised.docx -o redline.docx --author "Jane Doe"

bl docx changes redline.docx               # always JSON
bl docx comments redline.docx              # always JSON
bl docx check out.docx --original contract.docx

# Create
bl docx create --spec @spec.json -o new.docx
bl xlsx create --spec '{"sheets":[{"name":"S","rows":[["A","B"]]}]}' -o book.xlsx
bl pptx create --spec '{"slides":[{"texts":["Title","Body"]}]}' -o deck.pptx

# Other formats
bl xlsx view model.xlsx --sheet "Cap Table"
bl pptx edit deck.pptx --ops '[{"op":"set_text","slide":1,"element":1,"text":"Q3"}]' -o out.pptx

# Raw XML escape hatch
bl xml get file.docx word/document.xml --path body/p[0]/r[0]/t
bl xml eval file.docx word/document.xml 'count(//ins)'
bl xml select file.docx word/document.xml '//p[0]'
bl xml patch file.docx --part word/document.xml --ops '[{"op":"remove","sel":"//ins[0]"}]' -o out.docx
bl xml update file.docx --part word/document.xml --ops 'delete nodes //ins' -o out.docx
bl xml edit file.docx --ops '[{"part":"word/document.xml","action":"set_text","path":"body/p[0]/r[0]/t","text":"Hi"}]' -o out.docx
bl unpack file.docx unpacked/ && bl pack unpacked/ out.docx

# Synthetic test corpus
bl fixtures ./corpus

# Natural language — same CLI; needs `--features kalosm` to run a model
bl ai contract.docx "change thirty days to sixty days" \
    -o revised.docx --author "Jane Doe"
```

### Conventions

- **Outputs**: `-o/--output PATH` or `--in-place`. Never both.
- **JSON in** (`--ops`, `--spec`): inline JSON, `@file.json`, or `-` for stdin.
- **JSON out**: `--json` on reads; `info`, `check`, `changes`, `comments` always emit JSON.
- **Ranges**: `--from N --to N`, 1-based inclusive, same index space as `view`.
- **Exit codes**: `0` success · `1` operation failure · `2` usage error.
- **Author**: tracked changes and comments require `--author` or `BLACKLINE_AUTHOR`.

Edits are **strict** unless `--lenient` is passed. `--dry-run` validates without writing.

`--granularity` on `docx edit --track` and `docx redline`: `char` · `word` (default) · `sentence`.

### Natural language

`bl ai` is a subcommand on this CLI. A local Kalosm model emits the op
list; blackline applies it. Default model is quantized Phi-3.5 mini.
`cargo install blackline` stays lean; rebuild with `--features kalosm`
(plus `metal` or `cuda`) so the model runtime is present. The standalone
`blackline-ai` binary is the same pipeline. See [docs/ai.md](docs/ai.md).

```bash
bl ai contract.docx "change thirty days to sixty days" \
    -o revised.docx --author "Jane Doe"
```

## Library

```rust
use blackline_docx::{Docx, EditOp, Granularity, SearchQuery};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = Docx::from_paragraphs(&["The notice period is thirty (30) days."])?;

    let hits = doc.search(&SearchQuery::new("thirty").whole_word())?;
    assert_eq!(hits.total, 1);

    let outcome = doc
        .edit([EditOp::Replace {
            index: Some(1),
            old: "thirty (30)".into(),
            new: "sixty (60)".into(),
            content_match: None,
        }])
        .tracked("Jane Doe")
        .granularity(Granularity::Word)
        .apply()?;

    let edited = outcome.document.unwrap();
    edited.save("out.docx")?;
    assert!(edited.check_against(&doc)?.passed());
    Ok(())
}
```

Higher-level edits compile down to `TreeOp` mutations against a `Package` part.
`doc.package()` is public. Untouched parts stay byte-identical on save.

## Edit operations

**DOCX** — target by `index` (view index) or `match` (content anchor):

`replace` · `insert` · `delete` · `delete_run` · `format` ·
`table_insert_row` · `table_delete_row` · `insert_comment` ·
`delete_comment` · `accept_change` · `reject_change` ·
`accept_all` · `reject_all`

**XLSX** — `set_cell` · `set_range` · `insert_row` · `delete_row` ·
`insert_sheet` · `delete_sheet` · `set_sheet_name`

**PPTX** — `set_text` · `insert_slide` · `delete_slide`

**XML** — `set_attr` · `remove_attr` · `set_text` · `insert_child` ·
`remove_child` · `replace`

## Building & testing

```bash
cargo build --release          # binaries at target/release/{blackline,bl}
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Tests generate their own documents. `bl fixtures DIR` writes a
synthetic corpus (formatting, tables, tracked changes, comments,
formulas, multi-slide decks) to disk.

Real-world fixtures in [`tests/corpus/`](tests/corpus/) are copied from
[Apache POI](https://github.com/apache/poi) (Apache License 2.0). See
[`tests/corpus/NOTICE`](tests/corpus/NOTICE),
[`tests/corpus/LICENSE`](tests/corpus/LICENSE), and
[`tests/corpus/README.md`](tests/corpus/README.md).

| Corpus | Source |
| --- | --- |
| [`tests/corpus/docx/`](tests/corpus/docx/) | [POI `test-data/document`](https://github.com/apache/poi/tree/trunk/test-data/document) |
| [`tests/corpus/xlsx/`](tests/corpus/xlsx/) | [POI `test-data/spreadsheet`](https://github.com/apache/poi/tree/trunk/test-data/spreadsheet) |
| [`tests/corpus/pptx/`](tests/corpus/pptx/) | [POI `test-data/slideshow`](https://github.com/apache/poi/tree/trunk/test-data/slideshow) |

License text: [`tests/corpus/LICENSE`](tests/corpus/LICENSE)
([Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0)).

Requires Rust 1.88+. See [CONTRIBUTING.md](CONTRIBUTING.md).
Versioning and tags: [docs/releasing.md](docs/releasing.md).

## Layout

```
crates/
├── blackline-core/    # package, xml, tree, formula, patch, update, diff
├── blackline-docx/    # document façade, edits, revisions, track, comments
├── blackline-xlsx/    # workbook, cells, shared strings
├── blackline-pptx/    # presentation, slides
├── blackline-ai/      # local AI frontend (prompt → blackline ops)
└── blackline/         # bins blackline + bl
docs/                  # architecture, CLI, ai, releasing, going public
```

## License

MIT — see [LICENSE](LICENSE). Copyright (c) 2026 Vector Legal Systems, PBC.

Vendored test documents under [`tests/corpus/`](tests/corpus/) remain
Apache License 2.0, © The Apache Software Foundation, as copied from
the POI sources listed above. Full text:
[`tests/corpus/LICENSE`](tests/corpus/LICENSE). Attribution:
[`tests/corpus/NOTICE`](tests/corpus/NOTICE).
