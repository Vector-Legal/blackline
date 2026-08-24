# blackline-llm

Natural-language frontend for blackline. A local Kalosm model emits ops;
blackline applies them as native OOXML. The model never writes XML.

```
blackline-llm FILE INSTRUCTION
```

Install (Kalosm is opt-in so the rest of the workspace stays lean):

```
cargo install blackline-llm --features kalosm          # CPU, quantized
cargo install blackline-llm --features kalosm,metal    # Apple Silicon
cargo install blackline-llm --features kalosm,cuda     # NVIDIA
```

## Conventions

Same as [cli.md](cli.md): `-o` or `--in-place`, `--json`, `--dry-run`,
`--author` / `BLACKLINE_AUTHOR` for DOCX redlines, exit `0` / `1` / `2`.

PDF, Markdown, HTML, and plain text are refused. blackline edits the
XML in a `.docx` / `.xlsx` / `.pptx` package.

## Flags

```
blackline-llm FILE INSTRUCTION
    -o, --output PATH
        --in-place
        --author NAME
        --model NAME          phi-3.5 (default) | phi-3 | llama3.2-1b |
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
```

`INSTRUCTION` may be `@file` or `-` (stdin).

## Default model

**Phi-3.5 mini 4k instruct**, quantized, via Kalosm
(`LlamaSource::phi_3_5_mini_4k_instruct`). Small enough for a 16 GB
MacBook, trained for instruction following, which is what constrained
generation needs. `--model` overrides the preset or points at a GGUF.

First run downloads the GGUF into the Kalosm cache. Later runs are local.

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
