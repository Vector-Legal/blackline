# Architecture

Unopinionated Office Open XML toolkit. The library edits the XML that
actually lives in a `.docx` / `.xlsx` / `.pptx` package. It does not convert
documents to HTML, Markdown, or any other intermediate format.

---

## Layers

```
CLI  (blackline / bl)          JSON-first, noun-verb, agent-shaped
  │
format crates                  Docx / Xlsx / Pptx façades
  │
OOXML operations               body addressing, runs, cells, slides,
                               tracked changes, comments, redline
  │
blackline-core
  ├── Package                  ZIP of named parts, lossless untouched bytes
  ├── Relationships            .rels as typed records
  ├── ContentTypes             [Content_Types].xml
  ├── XmlDocument / XmlNode    parse, serialize, condense
  ├── TreeOp                   path-addressed XML mutations (least opinion)
  ├── Formula / Selector       query only — locate, do not mutate
  ├── Patch                    RFC 5261 add/replace/remove → TreeOp
  └── Update                   XQuery Update verbs → TreeOp
```

Higher-level edits (replace a phrase, wrap a tracked insertion, set a cell)
are compositions of `TreeOp` against a `Package` part.

This is the same stack used by SuperDoc (typed ops on native OOXML),
python-docx / python-pptx (OPC `Package` / `Part` / rels + xmlchemy),
docx.js (`XmlComponent` tree), and openpyxl (workbook zip + shared strings).

---

## XML

`XmlNode` is the document. Operations address a node by a path:

```
body/p[2]/r[0]/t          local-name + occurrence (0-based)
0/1/3                     raw child indices
```

`TreeOp` is the unopinionated mutation set:

```
set_attr / remove_attr / set_text
insert_child / remove_child / replace / rename
```

Anything an agent can express as "change this XML" can be a `TreeOp`.
Every higher XML layer compiles down to these. The CLI exposes each
layer on its own verb so opinionation is opt-in.

### Layering (opinionation goes up)

```
xml edit     TreeOp JSON              least opinion — raw path + verb
xml eval     Formula                  query only
xml select   Selector                 query only
xml patch    RFC 5261                 unique sel + add/replace/remove
xml update   XQuery Update            multi-node verbs, statement order
docx/xlsx/pptx edit                   format-specific, most opinion
track apply / track redline           most opinion — multi-author redline recipe
```

`formula`, `patch`, and `update` are separate modules. Neither patch
nor update depends on the other. Both depend on `formula` (to locate)
and `tree` (to mutate).

### Formulas

`blackline_core::formula` locates. It does not mutate.

| Source | Taken |
| --- | --- |
| [RFC 5261](https://www.rfc-editor.org/rfc/rfc5261.html) XML Patch | Location paths: `/`, `//`, `.`, `..`, `*`, `[@attr="value"]`, `/@attr`. |
| [XPath 1.0](https://www.w3.org/TR/xpath-10/) | `count`, `contains`, `starts-with`, `not`, `name` / `local-name`, `concat`, `substring`, `substring-before` / `after`, `string-length`, `normalize-space`, `boolean`, `number`. |
| SuperDoc | Query, then mutate. Formulas are the query. |

`substring` start is 0-based (same as `NodePath`), not XPath 1.0.
`[last()]` is the last remaining node of that step.

We did not vendor XPath 3.1 (xee, Saxon, libxml). Tree-edit distance
(X-Diff, XyDiff, Zhang–Shasha) and DaisyDiff-style word redlines stay
in `diff` / format redline code.

### Patch (RFC 5261)

`blackline_core::patch` is the IETF verb set. Each `sel` must match
**exactly one** node. Apply is sequential. Input is JSON or a `<diff>`
document. Namespace-axis `type="namespace::…"` is out of scope.

### Update (XQuery Update)

`blackline_core::update` is insert / delete / replace / replace-value /
rename. Statements run in order. `delete nodes` of many matches removes
last-to-first. Zero delete matches is a no-op. Input is JSON or:

```text
delete nodes //ins
insert node <w:p/> as first into /body
replace value of node //t[0] with "Hi"
rename node //p[0] as w:q
```

`--dry-run` on `xml patch` / `xml update` prints the compiled `TreeOp`s
and writes nothing.

**Indexing is 0-based**, matching `NodePath`, not XPath `position()`.
Names match local name (`ins` = `w:ins`).

```text
count(//ins)
exists(//del) or exists(//ins)
text(//p[0])
//ins/@author
contains(text(//delText), "thirty")
/body/p[0]/r[0]/t
//hyperlink[@id="rId4"]
```

CLI: `bl xml eval` · `xml select` · `xml patch` · `xml update` · `xml edit`.

---

## Package

```rust
let mut pkg = Package::open("file.docx")?;
let xml = pkg.part_xml("word/document.xml")?;
pkg.set_part_xml("word/document.xml", &xml)?;
pkg.save("out.docx")?;
```

Untouched parts are copied byte-for-byte. Only replaced parts are
re-serialized. That is the OOXML-preservation guarantee.

`Docx`, `Xlsx`, and `Pptx` are thin façades: they own a `Package`, know
which parts matter, and expose view / find / edit / create / check. They
do not hide the package. `doc.package()` is public.

---

## CLI

Canonical grammar: `blackline <format> <verb>` plus package verbs.

```
docx   view | outline | info | find | edit | create | check |
       changes | comments | redline | cat | parts
xlsx   view | info | find | edit | create | check | cat | parts
pptx   view | info | find | edit | create | check | cat | parts
xml    get | eval | select | edit | patch | update
track  apply | redline | changes | comments | settle
ai     FILE INSTRUCTION
unpack FILE DIR
pack   DIR FILE
fixtures DIR
```

See [cli.md](cli.md) for the full command reference.

---

## Edit operations

**DOCX** — target by `index` (1-based view index) or JSON `match`
(Rust: `content_match`):

`replace` · `insert` · `delete` · `delete_run` · `format` ·
`table_insert_row` · `table_delete_row` · `insert_comment` ·
`delete_comment` · `accept_change` · `reject_change` ·
`accept_all` · `reject_all` · `set_hyperlink`

`track` is a separate module (`Docx::track` / `blackline track`):
`replace` · `insert` · `insert_paragraph` · `delete` · `comment` ·
`accept` · `reject` · `accept_all` · `reject_all`. Each op may carry
its own author and date. Replacements use `diff_minimal`. Deletes wrap
only the matched span.

`replace` splices visible text in place so hyperlinks, bookmarks,
comment markers, and other authors' `w:ins`/`w:del` stay put.
`set_hyperlink` wraps or retargets display text (`http:`, `mailto:`,
…). Header and footer stories are edited with `EditBuilder::part`
(CLI: `--part header|footer|word/header2.xml`).

**XLSX** — `set_cell` · `set_range` · `insert_row` · `delete_row` ·
`insert_sheet` · `delete_sheet` · `set_sheet_name`

**PPTX** — `set_text` · `insert_slide` · `delete_slide`

**XML** — query: `count` · `exists` · `text` · `attr` · `contains` ·
`concat` · `substring` · `normalize-space`. Selectors `//ins` /
`/@attr` / `[last()]`. RFC 5261 patch. XQuery Update. Mutations:
`set_attr` · `remove_attr` · `set_text` · `insert_child` ·
`remove_child` · `replace` · `rename`

---

## Testing

Most tests generate their own documents. Apache POI fixtures live in
`tests/corpus/{docx,xlsx,pptx}/` (see `tests/corpus/NOTICE` and
`tests/corpus/LICENSE`).

1. Unit tests on XML parse/serialize, tree paths, Package round-trip,
   tokenization, LCS, cell refs.
2. Library tests that create a file, mutate it, reopen it, and assert
   on extracted text **and** on the XML tree (`w:ins`/`w:del` nesting,
   shared-string indices, slide rels).
3. Corpus tests that open real Office files:
   - DOCX (`crates/blackline-docx/tests/corpus.rs`): edit, redline,
     reject-all / accept-all precision.
   - XLSX / PPTX (`crates/blackline-xlsx/tests/corpus.rs`,
     `crates/blackline-pptx/tests/corpus.rs`): surgical `set_cell` /
     `set_text`, reopen, sidecar byte identity. No redline API.
4. CLI tests for verbs, JSON shapes, exit codes, author policy,
   strict/lenient, dry-run, plus `e2e_docx.rs` / `e2e_xlsx.rs` /
   `e2e_pptx.rs` / `e2e_track.rs` against the same corpora. Track
   tests cover multi-author replace/insert/delete/comment, minimize
   (delete+insert of the same text cancels), surgical delete next to
   hyperlinks, and reject-all / accept-all precision.
5. `blackline fixtures DIR` writes a synthetic corpus covering
   formatting, tables, tracked changes, comments, formulas, and
   multi-slide decks.

Precision rule: reject-all on a tracked edit or redline must reproduce
the original visible text; accept-all must reproduce the new text.
Package check must pass after every successful edit. Two-document
redline on a file that already contains Word revisions must flatten
those revisions first.

---

## AI sector

`bl ai` lives in the CLI crate, not a format crate. It depends on the
format façades and does not add models, prompts, or conversion to
`blackline-core`.

```
prompt + numbered view (instruction hits, or --from/--to)  →  Plan JSON  →  Docx::track / Xlsx::edit / Pptx::edit
```

`--from` / `--to` is optional. When omitted, `bl ai` searches the file
for phrases in the instruction and only those hits plus a neighbor go
to the model. `every paragraph` / `throughout the document` walks the
file in chunks. Prompt lines are abbreviated so the model copies a
short `old`, not a whole legal paragraph. Ops still carry the original
1-based view index.

On macOS `--features metal` the runtime is llama.cpp Metal (all layers
on the GPU). Elsewhere it is Kalosm (constrained `Plan`, JSON fallback).
`Completer` is the only extra type. Tests inject a canned plan so CI
never downloads a GGUF. Default model is quantized Phi-3 mini 4k;
`--model` overrides. PDF and Markdown stay out of scope. The model
handle is dropped after the plan is parsed. There is no resident model.
`bl ai --clear-cache` deletes the on-disk GGUFs.

See [ai.md](ai.md).

---

## Historical notes (v3 rewrite)

Earlier versions of this toolkit were coupled to a legal-drafting
workflow (financing aliases, defined-term conventions, section-reference
audits, signature-matrix parsing, PDF assembly). Those are useful
products. They are the wrong foundation for a general Office XML library.

v3 rebuilt around OPC + XML primitives and dropped PDF, legal-only
modules, and legacy CLI aliases. Diff, dates, and directory walks are
implemented in-tree so the dependency set stays small
(`quick-xml`, `zip`, `serde`, `clap`, `thiserror`, `tempfile`).
