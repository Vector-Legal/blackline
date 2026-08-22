# Versioning and release

blackline is a Cargo workspace with a **single shared version**
(`workspace.package.version` in the root `Cargo.toml`). All crates
(`blackline-core`, `blackline-docx`, `blackline-xlsx`, `blackline-pptx`,
`blackline-cli`) ship together.

## SemVer

Until `1.0.0`, treat `0.x` as:

- **patch** (`0.3.1`) — bug fixes, docs, tests; no public API breaks
- **minor** (`0.4.0`) — new ops, new CLI verbs, or breaking API changes

After `1.0.0`, follow [SemVer](https://semver.org): breaking changes bump
major.

Git tags are `vMAJOR.MINOR.PATCH` (for example `v0.3.0`). The `v` prefix
is required — the Release workflow matches `v*.*.*`.

## Cut a release

1. Update `CHANGELOG.md` (move `[Unreleased]` into `[X.Y.Z] — YYYY-MM-DD`).
2. Set `version` in the root `Cargo.toml` `[workspace.package]` table.
3. Set the same version on the four `blackline-*` path dependencies in
   `[workspace.dependencies]` (required for `cargo publish`).
4. Commit: `Release vX.Y.Z`.
5. Tag and push:

   ```bash
   git tag -a vX.Y.Z -m "vX.Y.Z"
   git push origin HEAD
   git push origin vX.Y.Z
   ```

6. Pushing the tag runs [`.github/workflows/release.yml`](../.github/workflows/release.yml):
   tests, then a GitHub Release with notes from the tag annotation and
   CHANGELOG.

## crates.io (optional)

Publishing is **not** automatic. First public publish, name
reservation, Trusted Publishing, and GitHub settings:
[going-public.md](going-public.md).

When a version is ready:

```bash
cargo publish -p blackline-core
cargo publish -p blackline-docx
cargo publish -p blackline-xlsx
cargo publish -p blackline-pptx
cargo publish -p blackline-cli
```

Publish `blackline-core` first. The format crates depend on it; the CLI
depends on all four. Inspect the uploaded file set first with
`cargo package --list -p CRATE --no-verify`.

A publish is permanent. `cargo yank` stops new dependents from picking
that version; it does not delete the crate or any secret that shipped
in it.

For CI, prefer [Trusted Publishing](https://crates.io/docs/trusted-publishing)
(OIDC, 30-minute token) over a long-lived `CARGO_REGISTRY_TOKEN`. The
first version of each crate still has to be published by hand — crates.io
cannot attach a trusted publisher to a name that does not exist yet.

## Moving the repository

The GitHub URL lives in one place: `workspace.package.repository` (and
`homepage`) in the root `Cargo.toml`. Update those two fields, the badge
URLs in `README.md`, the compare links at the bottom of `CHANGELOG.md`,
and this document's examples. Workflows use `github.repository` and do
not hard-code an owner or name.

Suggested GitHub topics after the public transfer: `rust`, `ooxml`,
`docx`, `xlsx`, `pptx`, `office`, `cli`.
