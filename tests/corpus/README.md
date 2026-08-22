# Test corpus

Vendored Office Open XML files used by the crate and CLI end-to-end tests.
Apache License 2.0 — see `LICENSE` and `NOTICE` in this directory.

ClusterFuzz, password-protected, binary OLE2 (`.doc` / `.xls` / `.ppt`),
and files that need Git LFS are not included. Everything here stays under
~1 MB.

## `docx/`

Fifteen `.docx` files from [Apache POI](https://github.com/apache/poi)
`test-data/document` (Apache License 2.0). See `NOTICE` and `LICENSE`.

Selected for structural diversity (body text, headings, tables, lists,
headers, comments, existing tracked changes, fields, footnotes, pictures,
bookmarks).

| File | Why it is here |
| --- | --- |
| `sample.docx` | Long body text |
| `SampleDoc.docx` | Multi-page styled body |
| `TestDocument.docx` | Mixed runs and a hyperlink |
| `heading123.docx` | Heading outline |
| `Styles.docx` | Heading + body styles |
| `TestTableCellAlign.docx` | Table cells |
| `Numbering.docx` | Numbered lists |
| `ComplexNumberedLists.docx` | Nested numbered lists |
| `HeaderFooterUnicode.docx` | Body plus Unicode header/footer |
| `testComment.docx` | Word comments (range start/end + reference) |
| `delins.docx` | Existing `w:ins` / `w:del` revisions |
| `FieldCodes.docx` | Field codes |
| `footnotes.docx` | Footnotes |
| `VariousPictures.docx` | Embedded pictures (untouched parts) |
| `bookmarks.docx` | Bookmarks |

## `xlsx/`

Fifteen `.xlsx` files from POI `test-data/spreadsheet`.

XLSX has no tracked-change API. Corpus tests open, view, `check`, set a
far-away cell, reopen, and assert sidecar parts (charts, drawings,
comments, theme, media) keep their original bytes.

| File | Why it is here |
| --- | --- |
| `SampleSS.xlsx` | Multi-sheet sample with styled shared strings |
| `Formatting.xlsx` | Number and date formats |
| `InlineString.xlsx` | Inline strings and omitted cell `r` attributes |
| `SimpleWithComments.xlsx` | Cell comments + VML drawing |
| `shared_formulas.xlsx` | Shared formulas |
| `formula-eval.xlsx` | Cached formula results |
| `WithTable.xlsx` | Excel table |
| `WithChart.xlsx` | Chart + drawing parts |
| `WithConditionalFormatting.xlsx` | Conditional formatting |
| `headerFooterTest.xlsx` | Header / footer in the worksheet |
| `TwoSheetsNoneHidden.xlsx` | Two visible sheets |
| `unicodeSheetName.xlsx` | Non-ASCII sheet name |
| `sharedhyperlink.xlsx` | Repeated hyperlink text |
| `ForShifting.xlsx` | Row insert / delete |
| `54084 - Greek - beyond BMP.xlsx` | Supplementary-plane Greek |

## `pptx/`

Fifteen `.pptx` files from POI `test-data/slideshow`.

`insert_slide` / `delete_slide` rebuild a deck from a text spec and are
not used against these files. Corpus edits are surgical `set_text` on an
existing `txBody`, or a lossless save/reopen when the slide has no text
frames (SmartArt, embedded audio).

| File | Why it is here |
| --- | --- |
| `SampleShow.pptx` | Two-slide title / bullets |
| `sample.pptx` | Body text plus notes slides |
| `present1.pptx` | Sparse text frames |
| `WithMaster.pptx` | Slide master / footer inheritance |
| `SmartArt.pptx` | Diagram parts (no slide `txBody`) |
| `table_test.pptx` | Empty table cells |
| `table-with-theme.pptx` | Themed table text |
| `bar-chart.pptx` | Chart part |
| `pie-chart.pptx` | Chart part |
| `with_japanese.pptx` | CJK, Gothic, footnote `#fragment` hyperlinks |
| `shapes.pptx` | Mixed shapes and table-like text |
| `layouts.pptx` | Many layout variants |
| `bug58144-headers-footers-2007.pptx` | Headers / footers |
| `45545_Comment.pptx` | Comments, notes, pictures |
| `EmbeddedAudio.pptx` | Embedded `mp3` media |
