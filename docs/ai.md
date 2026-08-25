# `bl ai`

Natural-language frontend for blackline. A local Kalosm model emits ops;
blackline applies them as native OOXML. The model never writes XML.

```
bl ai FILE INSTRUCTION
```

Kalosm is opt-in so a default `cargo install blackline` stays lean.
`bl ai --help` always works; running a prompt without the feature
prints the rebuild line.

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
        --strict              abort on the first failed op
        --granularity char|word|sentence
        --from N --to N       optional window; default is phrases from
                              the instruction
        --sheet NAME          XLSX
        --verbose             fetch, unload, and every apply op
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

On Apple Silicon (`--features metal`) inference is **llama.cpp Metal**:
every layer is offloaded to the GPU. Kalosm is only used to download
the GGUF. Candle Metal is not used — it yields NaN logits
(`No token sampled`) even for TinyLlama. `n_ctx` is capped at 4096 so
a 128k GGUF does not size a 128k KV cache. Generation stops after the
plan token cap (1536 new tokens) or as soon as the JSON object closes.

Preset names (`phi-3`, `tinyllama`, …) always win over a file of the
same name in the current directory. Default stderr is a few status
lines (`view`, `loading model`, `planning N/M`, applied/failed).
`--verbose` adds cache path, fetch, unload, planned-op count, and
every apply op. If you still see `backend=kalosm` on a Mac, the
binary was not built with `--features metal`.

Apply is **best-effort** by default. A leftover model op is reported
and the rest still writes. That is the opposite of `bl docx edit`
(strict unless `--lenient`). Pass `--strict` to abort on the first
miss. `--json` is the full plan + apply report.

## Lifecycle

`bl ai` is one-shot. There is no resident daemon and no session that
outlives the command.

On a Metal Mac the llama.cpp context is dropped after the plan is
parsed. Elsewhere Kalosm's `Llama` is a **channel handle**: dropping
it closes the worker and frees the tensors before the package is
written. Process exit is the hard guarantee if anything is still
unwinding.

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
through `Docx::track` unless `--no-track`. You do not have to pass
`--from` / `--to`:

- `change thirty days to sixty days` looks up that phrase
- `change title to …` is the first paragraph
- `update every paragraph …` / `throughout the document` walks the
  whole file in chunks of at most 12 lines (the GGUF stays loaded)

Each prompt line is abbreviated. The model must copy a **short** `old`
(a few words) and the number before `|` as `index` — not 1..N of the
window, and not the whole paragraph. `--from` / `--to` remains an
explicit override.

## Pipeline

```
FILE + INSTRUCTION
    → numbered view (instruction hits, document-wide chunks, or `--from`/`--to`)
    → JSON Plan (llama.cpp Metal, or Kalosm constrained / JSON)
    → snap (rewrite ops onto text that exists in the paragraph)
    → blackline track / edit
    → package check
```

There are two extra types: `Completer` (production is Kalosm / llama.cpp;
tests inject a canned plan) and `snap_plan` (Phi-3 ops → real spans).
No tool loop, no RAG, no conversion layer.
