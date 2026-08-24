# Versioning and release

blackline is a Cargo workspace with a **single shared version**
(`workspace.package.version` in the root `Cargo.toml`). All crates
(`blackline-core`, `blackline-docx`, `blackline-xlsx`, `blackline-pptx`,
`blackline-ai`, `blackline`) ship together.

## SemVer

Until `1.0.0`, treat `0.x` as:

- **patch** (`0.3.1`) — bug fixes, docs, tests; no public API breaks
- **minor** (`0.4.0`) — new ops, new CLI verbs, or breaking API changes

After `1.0.0`, follow [SemVer](https://semver.org): breaking changes bump
major.

Git tags are `vMAJOR.MINOR.PATCH` (for example `v0.3.0`). The `v` prefix
is required.

## Day-to-day work

**Never touch a version number in a normal pull request.** CI rejects it.

Land changes on `main` through pull requests and add user-visible entries
under `## [Unreleased]` in `CHANGELOG.md`. That section is the release: it
accumulates until somebody decides to cut a version.

## Cut a release

Releases are deliberate, and they run in two steps so that no workflow ever
pushes to `main` — branch protection stays intact and the version bump gets a
reviewable diff.

### 1. Prepare

**Actions → Prepare release → Run workflow**, pick `patch` / `minor` / `major`
(or type an explicit version).

It refuses to run off `main`, refuses an empty `[Unreleased]`, bumps the
version across all six crates and the `[workspace.dependencies]` entries,
rolls `[Unreleased]` into `[X.Y.Z] — YYYY-MM-DD`, and opens a
`release/X.Y.Z` pull request. Nothing is tagged or published yet.

### 2. Merge

Review the diff and merge. That is the point of no return.

`release-publish.yml` notices the version changed, re-runs the full
verification, packages all six crates, tags `vX.Y.Z`, publishes bottom-up
over OIDC, and cuts the GitHub Release. Publishing waits on the `crates-io`
environment, which requires an approval — so a merge alone does not ship.

Detection compares the workspace version against the previous commit rather
than parsing the commit message, so a hand-edited bump behaves identically.
Any push to `main` that does not change the version is a no-op, and a version
whose tag already exists is skipped.

### Doing it by hand

[`cargo-release`](https://github.com/crate-ci/cargo-release) drives the bump,
configured in [`release.toml`](../release.toml). Dry-run is the default:

```bash
cargo release patch                  # show what would happen
cargo release patch --execute        # bump, tag, push, publish
```

`shared-version` keeps all six crates on one version, `consolidate-commits`
puts the bump in one commit, and `tag-name = "v{{version}}"` gives one tag for
the workspace instead of cargo-release's per-crate default of
`blackline-core-v0.3.1`.

The changelog rewrite lives in
[`crates/blackline-core/release.toml`](../crates/blackline-core/release.toml)
rather than the root config: `pre-release-replacements` run once per package
with paths relative to that package, so a workspace-level rule would try to
rewrite the shared changelog five times and `exactly = 1` would reject the
second pass.

To prepare without releasing — what the workflow does in step 1:

```bash
cargo release patch --execute --no-publish --no-push --no-tag
```

### Things worth knowing

**Publishing six crates is not atomic.** If the third upload fails, the first
two are live and permanent — `cargo yank` hides a version but never deletes
it. Recovery is to fix forward and cut the next patch. Hence the full test
suite and a package of every crate before anything uploads.

**The first publish of each crate cannot be automated.** crates.io cannot
attach a trusted publisher to a name that does not exist, so version one of
each crate goes up by hand. See [going-public.md](going-public.md).

**Trusted Publishing must be configured per crate**, pointing at this
repository, `release-publish.yml`, and the `crates-io` environment. Without it
the publish step has no credentials. Do not add a long-lived
`CARGO_REGISTRY_TOKEN` secret.

**Versions never move in an ordinary pull request.** CI rejects it. Only the
release flow bumps.

### Publishing by hand

```bash
cargo publish -p blackline-core
cargo publish -p blackline-docx
cargo publish -p blackline-xlsx
cargo publish -p blackline-pptx
cargo publish -p blackline
```

`blackline-core` first: the format crates depend on it, and the CLI depends on
all four. Inspect the uploaded file set with
`cargo package --list -p CRATE --no-verify`.

## Moving the repository

The GitHub URL lives in one place: `workspace.package.repository` (and
`homepage`) in the root `Cargo.toml`. Update those two fields, the badge
URLs in `README.md`, the compare links at the bottom of `CHANGELOG.md`,
and this document's examples. Workflows use `github.repository` and do
not hard-code an owner or name.

Suggested GitHub topics after the public transfer: `rust`, `ooxml`,
`docx`, `xlsx`, `pptx`, `office`, `cli`.
