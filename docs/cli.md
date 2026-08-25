# CLI reference

`blackline` and `bl` are the same program. Grammar:

```
blackline <format> <verb> [args]
blackline ai FILE INSTRUCTION
blackline unpack FILE DIR
blackline pack DIR FILE
blackline fixtures DIR
```

## Conventions

| Topic | Rule |
|-------|------|
| Output | `-o/--output PATH` or `--in-place`. Never both. |
| JSON in | `--ops` / `--spec`: inline JSON, `@file.json`, or `-` (stdin). |
| JSON out | `--json` on reads. `info`, `check`, `changes`, `comments` always emit JSON. |
| Ranges | `--from N --to N`, 1-based inclusive, same index space as `view`. |
| Author | Tracked changes and comments require `--author` or `BLACKLINE_AUTHOR`. |
| Exit | `0` success · `1` operation failure · `2` usage error. |
| Strict | A failed edit op aborts the batch and writes nothing. `--lenient` applies what it can. |
| Dry-run | `--dry-run` validates without writing (no `-o` required). |

## `docx`

```
blackline docx view FILE [--from N] [--to N] [--raw] [--json]
blackline docx outline FILE [--json]
blackline docx find FILE QUERY [--case-sensitive] [--whole-word] [--limit N] [--from N] [--to N] [--json]
blackline docx info FILE...
blackline docx edit FILE --ops JSON [-o OUT | --in-place] [--track] [--author NAME]
                         [--granularity char|word|sentence] [--part header|footer|PATH]
                         [--lenient] [--dry-run] [--json]
blackline docx create --spec JSON -o OUT
blackline docx check FILE [--original FILE]
blackline docx changes FILE
blackline docx comments FILE
blackline docx redline ORIGINAL REVISED -o OUT [--author NAME] [--granularity char|word|sentence]
blackline docx cat FILE [PART]
blackline docx parts FILE [--json]
```

`--granularity` (default `word`) controls how tracked replacements and
redlines are chunked: `char` (tightest), `word`, or `sentence`.

`--part` applies the batch to a header/footer story (`header`, `footer`,
or `word/header2.xml`) instead of `word/document.xml`.

`set_hyperlink` wraps or retargets display text:

```json
{"op":"set_hyperlink","match":"normal","text":"normal","url":"mailto:docket@example.com"}
```

## `xlsx`

```
blackline xlsx view FILE [--sheet NAME|INDEX] [--json]
blackline xlsx info FILE
blackline xlsx find FILE QUERY [--limit N] [--json]
blackline xlsx edit FILE --ops JSON [-o OUT | --in-place] [--lenient] [--dry-run] [--json]
blackline xlsx create --spec JSON -o OUT
blackline xlsx check FILE
blackline xlsx cat FILE [PART]
blackline xlsx parts FILE [--json]
```

Sheets are addressed by name or 1-based index. Cells use A1 references.

## `pptx`

```
blackline pptx view FILE [--json]
blackline pptx info FILE
blackline pptx find FILE QUERY [--limit N] [--json]
blackline pptx edit FILE --ops JSON [-o OUT | --in-place] [--lenient] [--dry-run] [--json]
blackline pptx create --spec JSON -o OUT
blackline pptx check FILE
blackline pptx cat FILE [PART]
blackline pptx parts FILE [--json]
```

Text frames are 1-based `element` indices on a 1-based `slide`.

## `track`

Opinionated Word redline module on top of the CLI. Each op may name its
own author and date. Replacements run through `diff_minimal` (identical
delete+insert pairs cancel; shared prefix/suffix stays unmarked).
Deletes wrap only the matched span in `w:del`.

```
blackline track apply FILE --ops JSON [-o OUT | --in-place] [--author NAME] [--date ISO]
                           [--granularity char|word|sentence] [--part header|footer|PATH]
                           [--lenient] [--dry-run] [--json]
blackline track redline ORIGINAL REVISED -o OUT [--author NAME] [--granularity char|word|sentence]
blackline track changes FILE [--author NAME]
blackline track comments FILE [--author NAME]
blackline track settle FILE --accept|--reject [--author NAME] [-o OUT | --in-place] [--json]
```

`--author` (or `BLACKLINE_AUTHOR`) is the default for ops that omit one.
`--date` stamps `w:date` on revisions created by this batch.

```json
[
  {"op":"replace","match":"thirty (30)","old":"thirty (30)","new":"sixty (60)","author":"Jane"},
  {"op":"insert","match":"days","position":"before","text":"calendar ","author":"Bob"},
  {"op":"delete","match":"ALPHA","text":"ALPHA","author":"Jane"},
  {"op":"comment","match":"sixty (60)","anchor":"sixty (60)","text":"Confirm.","author":"Jane","date":"2026-01-15T12:00:00Z"},
  {"op":"accept_all","author":"Jane"},
  {"op":"reject_all","author":"Bob"}
]
```

`insert` positions: `before` / `after` (relative to `match`) or
`start` / `end` of the paragraph. `insert_paragraph` adds a whole
paragraph as `w:ins`. `delete` without `text` / `old` marks the whole
visible paragraph deleted (the node stays).

`track redline` uses the same minimize path. Reject-all of a successful
redline equals the original visible text; accept-all equals the revised
text.

## `xml`

```
blackline xml get FILE PART [--path PATH]
blackline xml eval FILE PART FORMULA
blackline xml select FILE PART SELECTOR
blackline xml edit FILE --ops JSON [-o OUT | --in-place] [--dry-run] [--lenient] [--json]
blackline xml patch FILE --ops JSON|XML --part PART [-o OUT | --in-place] [--dry-run] [--lenient] [--json]
blackline xml update FILE --ops EXPR|JSON --part PART [-o OUT | --in-place] [--dry-run] [--lenient] [--json]
```

Each verb is one layer. `edit` is raw `TreeOp`. `eval` / `select` only
query. `patch` is RFC 5261. `update` is XQuery Update. Both compile to
`TreeOp`; `--dry-run --json` shows the compiled ops and writes nothing.

`xml get` prints a part, or a single node when `--path` is given
(for example `body/p[0]/r[0]/t`).

`xml eval` / `xml select` run `blackline_core::formula`. Always JSON.
Indexing is 0-based, same as `--path` / `TreeOp`.

```
bl xml eval tracked.docx word/document.xml 'count(//ins)'
bl xml eval tracked.docx word/document.xml 'exists(//ins) and exists(//del)'
bl xml eval tracked.docx word/document.xml 'contains(text(//delText), "thirty")'
bl xml select file.docx word/document.xml '//hyperlink[@id="rId4"]'
bl xml patch file.docx --part word/document.xml --ops '[{"op":"remove","sel":"//ins[0]"}]' -o out.docx
bl xml update file.docx --part word/document.xml --ops 'delete nodes //ins' -o out.docx
```

`--part` can be omitted on `patch` / `update` when every JSON object
includes `part`. A bare selector is a formula that yields a node-set.
`xml select` requires that; `count` / `text` / `contains` belong on
`xml eval`.

`xml edit` applies a JSON array of `TreeOp`s. Each object must include
`part` plus one of:

```json
{"part":"word/document.xml","action":"set_text","path":"body/p[0]/r[0]/t","text":"Hi"}
{"part":"word/document.xml","action":"set_attr","path":"body/p[0]","name":"w:rsidR","value":"00"}
{"part":"word/document.xml","action":"remove_attr","path":"body/p[0]","name":"w:rsidR"}
{"part":"word/document.xml","action":"insert_child","path":"body","index":0,"xml":"<w:p/>"}
{"part":"word/document.xml","action":"remove_child","path":"body","index":0}
{"part":"word/document.xml","action":"replace","path":"body/p[0]","xml":"<w:p/>"}
```

## Package verbs

```
blackline unpack FILE DIR
blackline pack DIR FILE
blackline fixtures DIR
```

`unpack` pretty-prints XML parts. `pack` condenses them back into a
`.docx` / `.xlsx` / `.pptx`. `fixtures` writes the synthetic test corpus.

## Natural language

`bl ai` is a subcommand on this CLI. See [ai.md](ai.md).

```
blackline ai FILE INSTRUCTION -o OUT --author NAME
blackline ai --clear-cache
```

The model emits blackline ops; this CLI applies them. Default model is
quantized Phi-3.5 mini (`--model` overrides). `cargo install blackline`
stays lean; rebuild with `--features kalosm` (plus `metal` or `cuda`)
so the model runtime is present. The model is one-shot: loaded, used
for one plan, dropped before the write. `bl ai --clear-cache` deletes
the downloaded GGUFs. See [ai.md](ai.md#lifecycle).
