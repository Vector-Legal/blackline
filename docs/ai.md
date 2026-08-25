# `bl ai` / `bl llm`

Natural-language frontend for blackline. A local Kalosm model emits ops;
blackline applies them as native OOXML. The model never writes XML.
`bl llm` is the same verb as `bl ai` — same flags, same pipeline.

```
bl ai FILE INSTRUCTION
bl llm FILE INSTRUCTION
```

Kalosm is opt-in so a default `cargo install blackline` stays lean.
`bl ai --help` and `bl llm --help` always work; running a prompt without
the feature prints the rebuild line.

```
cargo install blackline --features kalosm          # CPU, quantized
cargo install blackline --features kalosm,metal    # Apple Silicon
cargo install blackline --features kalosm,cuda     # NVIDIA
```

## Conventions

Same as [cli.md](cli.md): `-o` or `--in-place`, `--json`, `--dry-run`,
`--author` / `BLACKLINE_AUTHOR` for DOCX redlines, exit `0` / `1` / `2`.

PDF, Markdown, HTML, and plain text are refused. blackline edits the
XML in a `.docx` / `.xlsx` / `.pptx` package.

## Flags

```
bl ai FILE INSTRUCTION
    -o, --output PATH
        --in-place
        --author NAME
        --model NAME          phi-3 (default) | phi-3.5 | llama3.2-1b |
                              llama3.2-3b | llama3.1-8b | qwen2.5-1.5b |
                              qwen2.5-3b | qwen2.5-7b | tinyllama |
                              ./path.gguf
        --dry-run
        --json
        --no-track            DOCX: silent edit, not a redline
        --lenient
        --granularity char|word|sentence
        --from N --to N       window the numbered view
        --sheet NAME          XLSX
        --verbose
        --clear-cache         delete downloaded GGUFs
```

`INSTRUCTION` may be `@file` or `-` (stdin).

## Default model

**Phi-3.1 mini 4k instruct**, quantized, via Kalosm
(`LlamaSource::phi_3_1_mini_4k_instruct`). The GGUF is actually 4k
context, which fits a 16 GB machine. Kalosm's older
`phi_3_mini_4k_instruct` pin 404s; `phi_3_5_mini_4k_instruct` points at
bartowski's Phi-3.5 Q4, whose metadata is **128k** — Metal then sizes
RoPE / KV from that and can take tens of GB. That preset stays
available as `--model phi-3.5`. `--model` also accepts a `.gguf` path.
A local GGUF that omits a tokenizer looks for `tokenizer.json` next to
the weights.

First run downloads the GGUF into the Kalosm cache. Later runs are local.

## Lifecycle

`bl ai` is one-shot. There is no resident daemon and no session that
outlives the command.

Kalosm's `Llama` is a **channel handle**, not the weights themselves.
The quantized tensors (DRAM, and Metal / CUDA buffers when those
features are on) live on a worker thread. Dropping the last handle
closes the channel; the worker exits and Rust `Drop`s the tensors.
That happens after the plan is parsed, before the package is written.
Process exit is the hard guarantee if anything is still unwinding.

The GGUF on disk is separate. Clear it without leaving Kalosm:

```
bl ai --clear-cache
```

That deletes `DATA_DIR/kalosm/cache` (or `$BLACKLINE_KALOSM_CACHE` if
set). The next `bl ai FILE INSTRUCTION` downloads again.

| OS | Typical path |
|----|----------------|
| macOS | `~/Library/Application Support/kalosm/cache` |
| Linux | `~/.local/share/kalosm/cache` |
| Windows | `%APPDATA%\kalosm\cache` |

`--model ./file.gguf` is yours; `--clear-cache` does not delete it.

## What the model is allowed to emit

| File | Ops |
|------|-----|
| `.docx` | `replace` `insert` `insert_paragraph` `delete` `comment` |
| `.xlsx` | `set_cell` |
| `.pptx` | `set_text` |

Indexes are 1-based, the same space as `blackline docx view`. Applied
through `Docx::track` unless `--no-track`.

## Pipeline

```
FILE + INSTRUCTION
    → numbered view (blackline)
    → Kalosm task, constrained to Plan
    → blackline track / edit
    → package check
```

There is one abstraction, `Completer`. Production is Kalosm. Tests inject
a canned plan. No tool loop, no RAG, no conversion layer.
