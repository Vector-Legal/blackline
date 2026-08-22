# blackline-pptx

Read, create, and edit PPTX presentations. Slides are OPC parts; text
lives in DrawingML `a:t` elements. This crate is AI-generated; see the
[repository README](https://github.com/Vector-Legal/blackline#blackline).

```toml
[dependencies]
blackline-pptx = "0.3"
```

```rust
use blackline_pptx::{CreateSpec, Pptx, SlideSpec};

let deck = Pptx::create(&CreateSpec {
    slides: vec![SlideSpec { texts: vec!["Title".into()], notes: None }],
})?;
# Ok::<(), blackline_pptx::PptxError>(())
```

- [docs.rs](https://docs.rs/blackline-pptx)
- [Repository](https://github.com/Vector-Legal/blackline)

License: MIT.
