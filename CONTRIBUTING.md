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
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps
```

Optional but recommended — the hooks run fmt and clippy before each commit,
so a push is not the first time you learn CI is unhappy:

```bash
pipx install pre-commit   # or: brew install pre-commit
pre-commit install
```

### What CI checks

Four jobs run on every pull request. All four must pass before merge.

| Job | What it runs |
|-----|--------------|
| `fmt · clippy · test` | `cargo fmt --check`, clippy with `-D warnings`, the test suite, rustdoc with `-D warnings` |
| `MSRV 1.88` | the test suite on the oldest supported toolchain |
| `deny · typos` | [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) (advisories, licences, bans, sources) and [`typos`](https://github.com/crate-ci/typos) |
| `semver-checks` | [`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks) against the newest published version |

Things that trip people up:

- **Lints are a workspace policy**, in `[workspace.lints]` in the root
  `Cargo.toml`. `unsafe_code` is forbidden outright. Do not add
  `#![allow(...)]` to dodge a lint without saying why in the pull request.
- **A new dependency must satisfy `cargo-deny`.** The licence allowlist in
  [`deny.toml`](deny.toml) is permissive-only — MPL, CC0 and bespoke licences
  fail deliberately, so that adopting one is a decision rather than an
  accident. Run `cargo deny check` locally before adding a dependency.
- **Breaking a public API fails `semver-checks`** unless the version says so,
  and versions only move at release. If your change is genuinely breaking,
  say so in the pull request and it will go out in a minor bump (pre-1.0) or
  a major one (after).
- `typos` skips the vendored POI corpus and the Word-generated XML under
  `crates/*/src/assets/`.

## Layout

```
crates/blackline-core/    OPC package, XML DOM, TreeOp, formula, patch, update
crates/blackline-docx/    WordprocessingML (including track / redline)
crates/blackline-xlsx/    SpreadsheetML
crates/blackline-pptx/    PresentationML
crates/blackline-llm/     local Kalosm frontend (optional `--features kalosm`)
crates/blackline/         blackline / bl binaries (`bl ai` is the same pipeline)
docs/                    architecture, CLI, llm, releasing, going public
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
- **Command examples in the READMEs are tested.**
  `crates/blackline/tests/readme.rs` extracts every `bl …` invocation from
  the fenced blocks in both READMEs and checks the subcommand exists and each
  long flag appears in that subcommand's `--help`. Adding an example that does
  not work fails the test suite, and so does renaming a flag without updating
  the docs. The "use it from a coding agent" block especially: agents run it
  verbatim and cannot tell when it is stale.

## Pull requests

- Keep the diff scoped to the change.
- Match the existing style (`cargo fmt` is the style guide).
- Do not add a dependency unless it is load-bearing. Prefer a few dozen
  lines of Rust over a new crate.

## Versioning and releases

**Never change `workspace.package.version` in a pull request.** CI rejects
it — this is enforced, not a convention. All six crates share one version
and move together, only at release.

Instead, note user-visible changes under `## [Unreleased]` in
`CHANGELOG.md`. That section *is* the next release: it accumulates until a
maintainer cuts a version.

Releasing is two deliberate steps, and needs write access:

1. **Actions → Prepare release**, pick `patch` / `minor` / `major`. It bumps
   the version, rolls `[Unreleased]` into the new version, and opens a
   `release/X.Y.Z` pull request. Nothing is tagged or published yet, so
   closing that PR is free.
2. **Merge it.** `Publish release` then re-verifies, tags `vX.Y.Z`, and waits
   for an approval on the `crates-io` environment before publishing the five
   crates and cutting the GitHub Release.

It refuses to release an empty `[Unreleased]`, so a change with no changelog
entry cannot be shipped. Full detail, including the fact that publishing five
crates is not atomic, is in [docs/releasing.md](docs/releasing.md).
