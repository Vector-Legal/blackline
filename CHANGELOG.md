# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning follows [SemVer](https://semver.org) as described in
[docs/releasing.md](docs/releasing.md).

## [Unreleased]

### Added

- `bl ai` no longer requires `--from` / `--to`. Phrases in the
  instruction (`change thirty days to sixty days`, quoted spans,
  `change title to …` as the first paragraph) select the numbered
  view. `every paragraph` / `throughout the document` walks the file
  in chunks of at most 12 paragraphs. Truncated JSON keeps complete
  ops. `--from` / `--to` remains an explicit override.
- Before apply, `snap_plan` rewrites model ops onto text that exists
  in the paragraph: mashed ` | ` fields become one replace each,
  `insert` after/before becomes a same-length phrase replace when
  one exists, all-caps inserts become a first-word replace, and
  unsnappable leftovers (`H1:`, whitespace-only `old`, short
  leftover phrases) are dropped.

### Changed

- `bl ai` apply is best-effort by default. One leftover model op no
  longer throws away a finished plan. `--strict` restores abort-on-
  first-miss (the library `ApplyOptions` default is still strict).
  Default stderr is a short status: view size, one `loading model`
  line, in-place `planning N/M`, then applied/failed. `--verbose`
  prints cache fetch, unload, planned-op count, and every apply op.
  Apply errors no longer dump the planned-ops JSON.

### Fixed

- `bl ai` on Metal no longer dies with `No valid tokens were sampled`.
  Kalosm/Candle Metal left the sampler empty (NaN logits). On macOS
  `--features metal` now runs llama.cpp with every layer on the GPU
  and parses a JSON plan. Context allocation is capped at 4k so a
  128k GGUF does not size a 128k KV cache. Generation is capped at
  1536 new tokens (or earlier once the JSON object closes).
  `llama-cpp-2` and `llama-cpp-sys-2` are pinned to 0.1.154 so
  `cargo install --git` cannot pair an old wrapper with a newer C API.
  CPU still tries constrained generation first and falls back to JSON.
- `--model` preset names win over a same-named file in the current
  directory. A leftover `phi-3` file in Downloads used to load as a
  GGUF and could be the 128k Phi-3.5 weights (tens of GB on Metal).
- Default AI view cap is 2500 characters (was 16k), so TinyLlama 2k
  and Phi-3 4k are not overstuffed. When `--from` / `--to` is omitted,
  phrases in the instruction select the window.

## [0.4.1] — 2026-08-25

### Fixed

- Default `bl ai` model is Phi-3 mini 4k, not Phi-3.5. Kalosm's
  `phi_3_5_mini_4k_instruct` GGUF is 128k context; on Metal that
  sized caches to tens of GB and Mirostat then failed with
  `A weight is invalid in distribution`. Constrained generation
  now uses greedy sampling. `--model phi-3.5` still exists.
  The older `phi_3_mini_4k_instruct` Hugging Face pin 404s; the
  default now uses `phi_3_1_mini_4k_instruct`. A local `.gguf` that
  omits a tokenizer looks for a sibling `tokenizer.json`.

## [0.4.0] — 2026-08-25

### Added

- `bl ai FILE INSTRUCTION`: a local AI frontend on the main CLI that
  turns a natural-language prompt into blackline ops. Default model is
  quantized Phi-3.5 mini; `--model` selects a preset or a GGUF. DOCX
  ops apply as Word tracked changes. PDF / Markdown are refused — the
  toolkit still does not convert. `cargo install blackline` stays lean;
  rebuild with `--features kalosm` (plus `metal` or `cuda`). The model
  is dropped after the plan is parsed. `bl ai --clear-cache` deletes the
  downloaded GGUFs.

## [0.3.2] — 2026-08-23

### Fixed

- The `blackline` crate README pointed its API-docs link at
  `docs.rs/blackline`, which documents only the thin CLI wrapper. It now
  points at `docs.rs/blackline-core`, where the library actually is.

## [0.3.1] — 2026-08-23

### Added

- README instructions that can be pasted straight into a coding agent
  (Claude Code, Codex, Cursor, …) to install the CLI and drive it correctly:
  the noun-verb grammar, which commands emit JSON, 1-based indices, the
  `--author` requirement, and the strict-by-default edit behaviour. Included
  in the `blackline` crate README so it also appears on crates.io.

## [0.3.0] — 2026-08-22

Rewrite around OPC + XML primitives. The XML in the package is the document.

### Added

- In-memory `Package` with lossless untouched-part bytes, relationships,
  and `[Content_Types].xml` as first-class types.
- Path-addressed `TreeOp` mutations (`set_attr`, `set_text`, `insert_child`, …).
- Thin `Docx` / `Xlsx` / `Pptx` façades: view, find, edit, create, check.
- Agent-shaped CLI: `blackline <format> <verb>` plus `xml`, `unpack`,
  `pack`, and `fixtures`.
- Tracked changes (`w:ins` / `w:del`) with required authorship and
  char / word / sentence granularity.
- Two-document redline and comment insert/delete.
- Synthetic fixture generator (`bl fixtures DIR`).
- README / CONTRIBUTING note that this repository is AI-generated.
- Public-crate metadata (docs.rs URLs, per-crate MIT `LICENSE`, crates.io
  install badges) and a [going-public checklist](docs/going-public.md).
  The POI corpus now vendors the official Apache-2.0 text
  (`tests/corpus/LICENSE`) next to `NOTICE`.
- Track module on top of the CLI (`blackline track`). Multi-author
  replace / insert / surgical delete / custom comments, plus
  `diff_minimal` so a delete+insert of the same text cancels and
  replacements keep shared prefix/suffix unmarked. CLI:
  `track apply` / `track redline` / `track changes` / `track comments`
  / `track settle`. Library: `Docx::track`, `TrackOp`, `track_redline`.
- XML formulas in `blackline-core` (`formula`): RFC 5261 / XPath-subset
  selectors (`//ins`, `/body/p[0]`, `[@attr="value"]`, `/@attr`,
  `[last()]`) and XPath 1.0 functions (`count`, `exists`, `text`,
  `attr`, `name`, `contains`, `starts_with`, `ends_with`, `concat`,
  `substring`, `substring-before` / `after`, `string-length`,
  `normalize-space`, `boolean`, `number`, `not`).
- RFC 5261 patch module (`patch`) and XQuery Update module (`update`).
  Both compile to `TreeOp` and are separate add-ons. CLI:
  `xml eval` / `xml select` / `xml patch` / `xml update` / `xml edit`.
  `--dry-run` prints compiled `TreeOp`s. `TreeOp::rename` is a new
  primitive. 0-based indexes match `NodePath`.
- GitHub Actions CI (fmt, clippy, tests, rustdoc, MSRV 1.88) and a
  tag-driven GitHub Release workflow.
- Contributor, security, and release documentation for a public crate.
- Fifteen Apache POI `.docx` fixtures under `tests/corpus/docx/` and
  library/CLI tests that edit and redline them (reject-all = original,
  accept-all = revised).
- Fifteen Apache POI `.xlsx` and fifteen `.pptx` fixtures under
  `tests/corpus/xlsx/` and `tests/corpus/pptx/`, with library and CLI
  tests for open / view / check / surgical edit / reopen. Sidecar
  parts (charts, drawings, comments, media, notes, diagrams) stay
  byte-identical. README lists the POI `test-data` directories those
  files were copied from.
- Relationship `check` treats `#fragment` hyperlink targets
  (`#_ftn1`) as in-document anchors, not missing package parts.
- XLSX reads omitted SpreadsheetML cell `r` attributes as the next
  implicit A1-style address. Creating `xl/sharedStrings.xml` now
  registers the workbook relationship and content-type override.
- PPTX `set_text` replaces the whole text frame (first `a:t` gets the
  new string; sibling runs in that `txBody` are cleared).
- Structure-preserving DOCX replace: hyperlinks, bookmarks, multi-run
  formatting, and other authors' tracked changes survive a nearby edit.
- `set_hyperlink` op (`http:` / `mailto:` / other linkouts) and
  `Docx::hyperlinks()`.
- Edit headers/footers via `EditBuilder::part("header"|"footer"|path)`
  and `bl docx edit --part`.
- Two-document redline now diffs table cells and keeps nearby hyperlinks
  when the change is a surgical text splice.
- Comment `check` fails when `comments.xml` has orphans with no body
  markers.

### Changed

- The CLI crate is published as **`blackline`**, not `blackline-cli`
  (`cargo install blackline`). Binaries are unchanged: `blackline` and
  `bl`. crates.io names are first-come and permanent, so the bare name
  had to be claimed before the first publish rather than after.
- Bumped `quick-xml` 0.36 → 0.41 and `zip` 2 → 8.
- `zip` is built with only the `deflate` codec. OOXML packages are ZIP
  with deflate or stored entries, so the default `aes-crypto`, `bzip2`,
  `deflate64`, `lzma`, `ppmd`, `xz` and `zstd` features are off. That
  drops six transitive crates and shrinks the binary.

### Fixed

- XML text parsing no longer drops entity references. quick-xml 0.37
  began emitting entities as their own `GeneralRef` events instead of
  inlining them into the adjacent text event, and the parser's catch-all
  arm silently discarded them — so `Smith &amp; Wesson` round-tripped as
  `Smith  Wesson`. Entities are now resolved explicitly and adjacent text
  runs merge into a single node.

### Removed

- PDF crate and all PDF commands.
- Legal-only modules (financing aliases, defined-term convention, section
  refs, `restyle_phrase`, signature-matrix parser, `verify-refs`).
- Legacy v1 CLI aliases.
- Non-load-bearing dependencies (`similar`, `strsim`, `walkdir`, `rand`,
  `regex`, `chrono`, `base64`).

[Unreleased]: https://github.com/Vector-Legal/blackline/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/Vector-Legal/blackline/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/Vector-Legal/blackline/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/Vector-Legal/blackline/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/Vector-Legal/blackline/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Vector-Legal/blackline/releases/tag/v0.3.0
