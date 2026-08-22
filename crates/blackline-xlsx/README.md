# blackline-xlsx

Read, create, and edit XLSX workbooks. Cells, shared strings, and sheets
are ordinary OPC parts plus SpreadsheetML. This crate is AI-generated;
see the
[repository README](https://github.com/Vector-Legal/blackline#blackline).

```toml
[dependencies]
blackline-xlsx = "0.3"
```

```rust
use blackline_xlsx::Xlsx;

let wb = Xlsx::from_rows("Sheet1", &[vec!["hello", "world"]])?;
# Ok::<(), blackline_xlsx::XlsxError>(())
```

- [docs.rs](https://docs.rs/blackline-xlsx)
- [Repository](https://github.com/Vector-Legal/blackline)

License: MIT.
