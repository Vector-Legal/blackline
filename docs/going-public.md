# Going public

Checklist for the first public GitHub repository and the first
crates.io publish. Do this from a **new repository with a clean
history** — do not flip the existing private repo public.

Guides this list is distilled from:

- [Cargo Book: Publishing on crates.io](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [The Rust Book, ch. 14.2](https://doc.rust-lang.org/book/ch14-02-publishing-to-crates-io.html)
- [Cargo.toml conventions](https://doc.rust-lang.org/style-guide/cargo.html)
- [Rust API Guidelines — C-METADATA](https://rust-lang.github.io/api-guidelines/documentation.html#cargotoml-includes-all-common-metadata-c-metadata)
- [crates.io Trusted Publishing](https://crates.io/docs/trusted-publishing)
- [OpenZeppelin rust-project-template](https://github.com/OpenZeppelin/rust-project-template)
- [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)

Version tags, SemVer, and the day-to-day release loop live in
[releasing.md](releasing.md).

## 0. Before you create the new repo

- [ ] Working tree is only this snapshot (no private history, no
      deleted v2 legal modules, no client Office files).
- [ ] Search the tree for tokens, keys, `.env`, and real emails.
      Example addresses in tests should stay on `example.com`.
- [ ] Confirm crate names are still free (hyphens and underscores
      collide on crates.io):

      ```bash
      for n in blackline blackline-core blackline-docx \
               blackline-xlsx blackline-pptx; do
        code=$(curl -sS -o /dev/null -w "%{http_code}" \
          "https://crates.io/api/v1/crates/$n")
        echo "$n $code"   # 404 = free
      done
      ```

      As of 2026-08-22 all five names returned 404.

The published CLI crate is **`blackline`** (`cargo install blackline`
→ binaries `blackline` and `bl`). The bare name was claimed by renaming
the CLI crate before the first publish; crates.io names are first-come
and permanent, so this could not have been done afterwards.

## 1. Create the empty public repository

Create an **empty** public repo (no README / license / `.gitignore`
from GitHub — those are already in this tree).

If the new URL is not `https://github.com/Vector-Legal/blackline`, update
these before the first commit on the new remote:

| Location | Fields |
|----------|--------|
| Root `Cargo.toml` | `workspace.package.repository`, `homepage` |
| `README.md` | CI / crates.io / docs.rs badge URLs |
| `CHANGELOG.md` | compare + release links at the bottom |
| Each crate `README.md` | repository / docs.rs links |
| `docs/cli.md`, this file | any hardcoded `Vector-Legal/blackline` URLs |
| `.github/ISSUE_TEMPLATE/config.yml` | `contact_links` |

Workflows use `github.repository` and do not hard-code the owner.

Then:

```bash
git init
git add .
git commit -m "Initial public release of blackline 0.3.0"
git branch -M main
git remote add origin git@github.com:OWNER/REPO.git
git push -u origin main
```

Do not copy `.git` from the private repo.

## 2. GitHub repository settings

On the new repo:

- [ ] **Description**: `Unopinionated Office Open XML toolkit for DOCX, XLSX, and PPTX`
- [ ] **Homepage**: leave blank, or the same GitHub URL until there is a site
- [ ] **Topics**: `rust` `ooxml` `docx` `xlsx` `pptx` `office` `cli`
- [ ] **License**: MIT (detected from `LICENSE`)
- [ ] **Private vulnerability reporting**: Settings → Code security →
      enable. `SECURITY.md` already points reporters here.
- [ ] **Secret scanning** + **push protection**
- [ ] **Branch protection** on `main`: require the `CI` workflow
      (fmt, clippy, tests, rustdoc, MSRV 1.88)
- [ ] **Discussions** — optional
- [ ] Confirm Dependabot is on (`.github/dependabot.yml` is already
      in the tree)

Community health files already in this snapshot:

`README.md` · `LICENSE` · `CODE_OF_CONDUCT.md` · `CONTRIBUTING.md` ·
`SECURITY.md` · issue templates · pull-request template ·
`tests/corpus/LICENSE` (Apache-2.0 for the POI fixtures) + `NOTICE`

## 3. crates.io account (one-time)

1. Log in at [crates.io](https://crates.io) with GitHub.
2. Verify the email on [Account Settings](https://crates.io/settings).
3. Create an API token with publish scope. Store it only in
   `~/.cargo/credentials.toml` via `cargo login`, or as a GitHub
   Actions secret named `CARGO_REGISTRY_TOKEN`. **Never commit it.**
4. A publish is permanent. You can `cargo yank` a broken version;
   that does **not** delete the code or any secret that shipped in it.

## 4. Pre-publish package check

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps
```

Then inspect what would upload (does **not** need the other crates
to exist on crates.io):

```bash
for p in blackline-core blackline-docx blackline-xlsx \
         blackline-pptx blackline; do
  echo "===== $p ====="
  cargo package --list -p "$p" --no-verify
done
```

Each package must include `LICENSE`, `README.md`, and `Cargo.toml`.
The Apache POI corpus under `tests/corpus/` is workspace-only and
must not appear in those lists. crates.io rejects `.crate` files
over 10 MB.

`cargo publish --dry-run` only works for **leaf** crates until the
dependencies exist on the registry. After `blackline-core` is live,
dry-run the format crates; after those are live, dry-run the CLI.

## 5. First publish (manual, bottom-up)

Cut a version first (`[Unreleased]` → `[0.3.1]` or keep `0.3.0` if
this *is* the first tagged public snapshot). See
[releasing.md](releasing.md). Then:

```bash
cargo publish -p blackline-core
cargo publish -p blackline-docx
cargo publish -p blackline-xlsx
cargo publish -p blackline-pptx
cargo publish -p blackline
```

`cargo publish` (Rust ≥ 1.66) waits until the crate is in the index,
so no manual sleep is required between those commands.

Verify:

- https://crates.io/crates/blackline
- https://docs.rs/blackline-core (rebuilds automatically)
- `cargo install blackline` → `blackline --version` / `bl --version`

## 6. Trusted Publishing (after the first manual publish)

crates.io cannot attach Trusted Publishing to a crate that does not
exist yet. After step 5:

On each of the five crates → Settings → Trusted Publishing, add:

| Field | Value |
|-------|-------|
| Repository owner | `Vector-Legal` |
| Repository name | `blackline` |
| Workflow filename | `release-publish.yml` |
| Environment | `crates-io` |

The workflow side is already wired:
[`release-publish.yml`](../.github/workflows/release-publish.yml) declares
`id-token: write`, runs the `crates-io` environment, and exchanges the OIDC
token with
[`rust-lang/crates-io-auth-action`](https://github.com/rust-lang/crates-io-auth-action).
Until Trusted Publishing is registered on each crate, that step has no
credentials and the publish fails.

Do **not** add a long-lived `CARGO_REGISTRY_TOKEN` secret. Revoke the manual
token used for the first publish once OIDC works.

## 7. Owners

```bash
# Extra human (full rights, including adding owners):
cargo owner --add github-handle -p blackline-core
# Team (publish/yank only):
cargo owner --add github:Vector-Legal:owners -p blackline-core
```

Repeat per crate, or add the team once and publish as that team.
crates.io needs the GitHub `read:org` scope to see teams.

## 8. What this snapshot already has

| Item | Status |
|------|--------|
| MIT `LICENSE` (workspace + each crate) | yes |
| `description` / `license` / `repository` / `homepage` / `readme` | yes |
| `documentation` (docs.rs per crate) | yes |
| `keywords` (≤ 5) / `categories` | yes |
| `rust-version` (MSRV 1.88) | yes |
| Crate-level rustdoc | yes |
| Keep a Changelog + SemVer policy | yes |
| CI: fmt, clippy `-D warnings`, tests, rustdoc, MSRV | yes |
| Tag-driven GitHub Release | yes |
| Dependabot | yes |
| Security / CoC / contributing / issue templates | yes |

## 9. Leave for later

- Auto-publish from CI (step 6)
- `cargo deny` / `cargo semver-checks` in CI
- This Week in Rust / r/rust announcement
- XLSX / PPTX redline (still a draft PR, not required for 0.3.x)
