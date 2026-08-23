# blackline

Agent-first CLI for DOCX / XLSX / PPTX. Installs two identical binaries,
`blackline` and `bl`. This crate is AI-generated; see the
[repository README](https://github.com/Vector-Legal/blackline#blackline).

```bash
cargo install blackline
bl docx view file.docx
```

## Use it from a coding agent

Every verb is noun-first, the inspection commands emit JSON, and exit codes
separate a failed operation (1) from bad usage (2). Paste this into Claude
Code, Codex, Cursor, or any agent with shell access:

````text
Install and use `blackline`, a CLI for editing DOCX / XLSX / PPTX by
operating on the OOXML inside the package (no HTML/Markdown/PDF conversion).

    cargo install blackline
    bl --version

Grammar is `bl <format> <verb> FILE [args]` — format is docx | xlsx | pptx —
plus `track`, `xml`, `unpack`, `pack` and `fixtures`.

Rules that matter:
- `info`, `check`, `changes`, `comments` and `--json` always emit JSON. Parse
  that, not the human-readable `view` output.
- Indices in edit and track ops are 1-based.
- Tracked changes and comments need `--author NAME`, or set BLACKLINE_AUTHOR.
- Edits are strict: if one op fails, nothing is written. `--dry-run` to test,
  `--lenient` for best-effort.
- Exit codes: 0 success, 1 operation failed, 2 usage error.
- Write with `-o OUT` or `--in-place`. JSON args take inline JSON,
  `@file.json`, or `-` for stdin.

Read:

    bl docx view contract.docx --from 1 --to 40
    bl docx outline contract.docx
    bl docx info contract.docx                  # JSON
    bl docx find contract.docx "Purchase Price" --whole-word --json

Edit, leaving Word-native tracked changes someone can accept or reject:

    bl track apply contract.docx \
      --ops '[{"op":"replace","index":1,"old":"thirty days","new":"sixty days"}]' \
      -o revised.docx --author "Jane Doe"

    bl docx changes revised.docx                # JSON: who changed what
    bl track settle revised.docx --accept -o final.docx
    bl track settle revised.docx --reject -o original.docx

Diff two documents into a redline:

    bl track redline original.docx revised.docx -o redline.docx --author "Jane Doe"

Verify after mutating, against the original:

    bl docx check revised.docx --original contract.docx

Spreadsheets and decks:

    bl xlsx info model.xlsx                     # sheet names are in `sheet_names`
    bl xlsx view model.xlsx --sheet "Cap Table"
    bl pptx edit deck.pptx --ops '[{"op":"set_text","slide":1,"element":1,"text":"Q3"}]' -o out.pptx

Escape hatch to raw XML:

    bl xml eval file.docx word/document.xml 'count(//ins)'
    bl xml select file.docx word/document.xml '//p[0]'

`bl fixtures ./corpus` writes sample DOCX/XLSX/PPTX files to experiment on.
Run `bl <format> --help` for the full verb list.
````

- [CLI reference](https://github.com/Vector-Legal/blackline/blob/main/docs/cli.md)
- [Repository](https://github.com/Vector-Legal/blackline)
- [API docs](https://docs.rs/blackline-core) — the library lives in `blackline-core`

License: MIT.
