# blackline-ai

Local AI frontend for [blackline](https://github.com/Vector-Legal/blackline).
A natural-language prompt becomes a small op list; blackline applies it as
native OOXML. The model never writes XML.

This crate is AI-generated; see the
[repository README](https://github.com/Vector-Legal/blackline#blackline).

The same pipeline is `bl ai` on the main CLI. This crate is the
library plus a standalone binary, if you want that install on its own.

```bash
cargo install blackline --features kalosm              # `bl ai`
cargo install blackline-ai --features kalosm          # standalone
cargo install blackline --features kalosm,metal        # Apple Silicon
cargo install blackline --features kalosm,cuda         # NVIDIA
```

```bash
bl ai contract.docx "change thirty days to sixty days" \
    -o revised.docx --author "Jane Doe"
```

DOCX ops land as Word tracked changes. `--author` (or `BLACKLINE_AUTHOR`)
is required, same as `bl track apply`. XLSX and PPTX are silent edits.

## What it is

blackline is unopinionated: it edits the XML that actually lives in a
`.docx` / `.xlsx` / `.pptx`. blackline-ai sits **on top** of that toolkit
and does one thing:

1. Render the file as a numbered view (`bl docx view` index space).
2. Run a local Kalosm model with constrained generation into a `Plan`.
3. Apply the plan through `blackline-docx` / `xlsx` / `pptx`.
4. Check the package.

No HTML, Markdown, or PDF conversion. Those are not Office packages.

## CLI

```
bl ai FILE INSTRUCTION
blackline-ai FILE INSTRUCTION
```

```
-o, --output PATH          write here
    --in-place             overwrite FILE
    --author NAME          required for DOCX redlines
    --model NAME           preset or a .gguf path (default: phi-3.5)
    --dry-run              print the plan, write nothing
    --json                 plan + apply report
    --no-track             DOCX: silent edit, not a redline
    --lenient              apply what can be applied
    --granularity word     char | word | sentence
    --from N --to N        window the view (1-based)
    --sheet NAME           XLSX sheet
    --verbose              log model load
```

`INSTRUCTION` is the prompt. `@file` reads a file; `-` reads stdin.

```bash
bl ai contract.docx "change thirty days to sixty days" \
    -o out.docx --author "Jane Doe"

bl ai contract.docx "flag the indemnity clause" \
    --dry-run --json --author Jane

bl ai model.xlsx "set B2 to 42" --in-place --model llama3.2-3b

bl ai deck.pptx "set the title to Q3" -o out.pptx
```

Exit codes match blackline: `0` success · `1` operation failed · `2` usage.

## Model

**Default is quantized Phi-3.5 mini** (`phi-3.5`): Kalosm's small
reasoning model, Q4 GGUF, comfortable on a 16 GB MacBook. First run
downloads the file into the Kalosm cache.

| `--model` | What Kalosm loads | Notes |
|-----------|-------------------|-------|
| `phi-3.5` | Phi-3.5 mini 4k instruct | **default** |
| `phi-3` | Phi-3 mini 4k instruct | slightly older |
| `llama3.2-1b` | Llama 3.2 1B Instruct | smallest |
| `llama3.2-3b` | Llama 3.2 3B Instruct | |
| `llama3.1-8b` | Llama 3.1 8B Instruct | wants a GPU |
| `qwen2.5-1.5b` | Qwen2.5 1.5B Instruct | |
| `qwen2.5-3b` | Qwen2.5 3B Instruct | |
| `qwen2.5-7b` | Qwen2.5 7B Instruct | wants a GPU |
| `tinyllama` | TinyLlama 1.1B Chat | |
| `./path.gguf` | that file | any GGUF Kalosm can load |

Metal and CUDA are feature flags, not model names. The same preset
runs on CPU, Metal, or CUDA depending on how you installed.

## Ops the model may emit

The vocabulary is smaller than blackline's full JSON:

| Format | Ops |
|--------|-----|
| DOCX | `replace` `insert` `insert_paragraph` `delete` `comment` |
| XLSX | `set_cell` |
| PPTX | `set_text` |

Indexes are 1-based, same as `bl docx view`. `old` / `anchor` text is
copied from the view, not paraphrased. Applied through `Docx::track`
(or `Docx::edit` with `--no-track`).

## Library

```rust
use blackline_ai::{run_with_completer, ApplyOptions, Op, Plan, StaticCompleter};

# async fn demo() -> Result<(), blackline_ai::AiError> {
let plan = Plan {
    ops: vec![Op::Replace {
        index: 1,
        old: "thirty".into(),
        new: "sixty".into(),
    }],
};
let report = run_with_completer(
    "contract.docx".as_ref(),
    "change thirty to sixty",
    Some("out.docx".as_ref()),
    ApplyOptions {
        author: Some("Jane Doe".into()),
        ..ApplyOptions::default()
    },
    &StaticCompleter::new(plan),
)
.await?;
assert_eq!(report.apply.failed, 0);
# Ok(())
# }
```

`Completer` is the only abstraction. Production uses Kalosm constrained
generation into `Plan`. Tests inject `StaticCompleter`.

## Build

```bash
cargo test -p blackline-ai                 # no Candle, no download
cargo build -p blackline-ai --features kalosm --release
```

Workspace CI runs the first line. Compiling Kalosm / Candle is opt-in.

- [docs.rs](https://docs.rs/blackline-ai)
- [CLI](https://github.com/Vector-Legal/blackline/blob/main/docs/ai.md)
- [Repository](https://github.com/Vector-Legal/blackline)

License: MIT.
