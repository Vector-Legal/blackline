# Contributing

Thanks for working on blackline. The project is a Cargo workspace of
Rust crates plus a small CLI. **This codebase is AI-generated** (written
with Cursor agents). By participating you agree to the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Prerequisites

- Rust 1.88 or newer (`rustup` recommended; `rust-toolchain.toml` pins
  `stable` plus `rustfmt` / `clippy`)

## Workflow

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three commands on every pull request.

## Layout

```
crates/blackline-core/    OPC package, XML DOM, TreeOp, formula, patch, update
crates/blackline-docx/    WordprocessingML (including track / redline)
crates/blackline-xlsx/    SpreadsheetML
crates/blackline-pptx/    PresentationML
crates/blackline/         blackline / bl binaries
docs/                    architecture, CLI, releasing, going public
```

## Tests

Most library tests generate their own documents. Real-world fixtures
live in `tests/corpus/{docx,xlsx,pptx}/` (Apache POI, Apache-2.0; see
`tests/corpus/NOTICE` and `tests/corpus/LICENSE`). Do not check in
other third-party Office files.

- Library tests should create or open a file, mutate it, reopen it, and
  assert on extracted text **and** on the XML tree.
- Tracked-change and redline tests (DOCX only): reject-all must
  reproduce the original visible text; accept-all must reproduce the
  new text. XLSX and PPTX have no redline API — do not invent one.
- After a successful edit, `check` must pass.
- Files that already contain Word `w:ins` / `w:del` must be flattened
  (`accept_all` or `reject_all`) before a two-document redline, or
  reject-all will also revert those older revisions.
- CLI tests live in `crates/blackline/tests/cli.rs` plus
  `e2e_docx.rs`, `e2e_xlsx.rs`, `e2e_pptx.rs`, and `e2e_track.rs`.
- Advanced DOCX cases (multi-author markup, hyperlinks, headers) live
  in `crates/blackline-docx/tests/advanced.rs`.

`bl fixtures DIR` writes the synthetic corpus used by humans and agents.

## Documentation

- User-facing behavior: root `README.md` and `docs/cli.md`.
- Internals: `docs/architecture.md`.
- Public Rust items need rustdoc comments (`///`).
- Note user-visible changes in `CHANGELOG.md` under `[Unreleased]`.

## Pull requests

- Keep the diff scoped to the change.
- Match the existing style (`cargo fmt` is the style guide).
- Do not add a dependency unless it is load-bearing. Prefer a few dozen
  lines of Rust over a new crate.

## Versioning

See [docs/releasing.md](docs/releasing.md). Do not bump
`workspace.package.version` in a feature PR; that happens at release.
