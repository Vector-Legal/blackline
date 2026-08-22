# blackline-docx

Read, search, edit, redline, track, comment, create, and validate DOCX
documents. The XML in the package is the document. This crate is
AI-generated; see the
[repository README](https://github.com/Vector-Legal/blackline#blackline).

```toml
[dependencies]
blackline-docx = "0.3"
```

```rust
use blackline_docx::Docx;

let doc = Docx::from_paragraphs(&["Hello"])?;
assert_eq!(doc.visible_text()?, "Hello");
# Ok::<(), blackline_docx::DocxError>(())
```

- [docs.rs](https://docs.rs/blackline-docx)
- [Repository](https://github.com/Vector-Legal/blackline)
- [CLI](https://github.com/Vector-Legal/blackline/blob/main/docs/cli.md)

License: MIT.
