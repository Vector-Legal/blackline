//! Synthetic Office files used as a test corpus and as `blackline fixtures DIR`.

use std::path::Path;

use blackline_docx::{
    CreateSpec, Docx, EditOp, Granularity, ParaProps, ParaSpec, RunProps, RunSpec, TableSpec,
};
use blackline_pptx::{CreateSpec as PptxSpec, Pptx, SlideSpec};
use blackline_xlsx::{CreateSpec as XlsxSpec, SheetSpec, Xlsx};

/// Write the corpus into `dir`. Returns the paths that were written.
pub fn generate(dir: &Path) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let mut written = Vec::new();

    write_docx(dir, &mut written)?;
    write_xlsx(dir, &mut written)?;
    write_pptx(dir, &mut written)?;

    Ok(written)
}

/// CLI entry.
pub fn run(dir: &Path) -> Result<i32, String> {
    let written = generate(dir)?;
    println!("wrote {} file(s) into {}", written.len(), dir.display());
    for w in &written {
        println!("  {w}");
    }
    Ok(0)
}

fn write_docx(dir: &Path, written: &mut Vec<String>) -> Result<(), String> {
    let save = |name: &str, doc: Docx, written: &mut Vec<String>| -> Result<(), String> {
        let path = dir.join(name);
        doc.save(&path).map_err(|e| e.to_string())?;
        written.push(path.display().to_string());
        Ok(())
    };

    save(
        "simple.docx",
        Docx::from_paragraphs(&["Hello world.", "Second paragraph.", "Third paragraph."])
            .map_err(|e| e.to_string())?,
        written,
    )?;

    save(
        "empty.docx",
        Docx::from_paragraphs(&["Placeholder."]).map_err(|e| e.to_string())?,
        written,
    )?;

    save(
        "headings.docx",
        Docx::create(&CreateSpec {
            font: None,
            paragraphs: vec![
                para_styled("Chapter One", "Heading1"),
                para("An opening paragraph under the first heading."),
                para_styled("Section 1.1", "Heading2"),
                para("Details live in this section."),
                para_styled("A subsection", "Heading3"),
                para("Nested heading text."),
                para_styled("Chapter Two", "Heading1"),
                para("A later chapter."),
            ],
            sections: Vec::new(),
            tables: Vec::new(),
        })
        .map_err(|e| e.to_string())?,
        written,
    )?;

    save(
        "formatted.docx",
        Docx::create(&CreateSpec {
            font: Some("Calibri".into()),
            paragraphs: vec![
                ParaSpec {
                    text: None,
                    style: None,
                    runs: Some(vec![
                        run_spec(
                            "Bold ",
                            Some(RunProps {
                                bold: Some(true),
                                ..Default::default()
                            }),
                        ),
                        run_spec(
                            "italic ",
                            Some(RunProps {
                                italic: Some(true),
                                ..Default::default()
                            }),
                        ),
                        run_spec(
                            "underline ",
                            Some(RunProps {
                                underline: Some(true),
                                ..Default::default()
                            }),
                        ),
                        run_spec(
                            "strike ",
                            Some(RunProps {
                                strike: Some(true),
                                ..Default::default()
                            }),
                        ),
                        run_spec(
                            "red",
                            Some(RunProps {
                                color: Some("FF0000".into()),
                                ..Default::default()
                            }),
                        ),
                    ]),
                    props: ParaProps::default(),
                },
                ParaSpec {
                    text: Some("Centered title".into()),
                    style: None,
                    runs: None,
                    props: ParaProps {
                        align: Some("center".into()),
                        ..Default::default()
                    },
                },
            ],
            sections: Vec::new(),
            tables: Vec::new(),
        })
        .map_err(|e| e.to_string())?,
        written,
    )?;

    save(
        "mixed_runs.docx",
        Docx::create(&CreateSpec {
            font: None,
            paragraphs: vec![ParaSpec {
                text: None,
                style: None,
                runs: Some(vec![
                    run_spec("The ", None),
                    run_spec(
                        "Purchase Price",
                        Some(RunProps {
                            bold: Some(true),
                            ..Default::default()
                        }),
                    ),
                    run_spec(" is ", None),
                    run_spec(
                        "$1,000,000",
                        Some(RunProps {
                            underline: Some(true),
                            ..Default::default()
                        }),
                    ),
                    run_spec(".", None),
                ]),
                props: ParaProps::default(),
            }],
            sections: Vec::new(),
            tables: Vec::new(),
        })
        .map_err(|e| e.to_string())?,
        written,
    )?;

    save(
        "special_chars.docx",
        Docx::from_paragraphs(&[
            r#"Ampersand & less-than < greater-than > quotes "double" and 'single'."#,
            "Smart quotes: “curly” and ‘single’ — plus an em-dash.",
            "Unicode: café, naïve, 日本語, Ελληνικά, emoji 📎.",
        ])
        .map_err(|e| e.to_string())?,
        written,
    )?;

    save(
        "table.docx",
        Docx::create(&CreateSpec {
            font: None,
            paragraphs: vec![para("Inventory")],
            sections: Vec::new(),
            tables: vec![TableSpec {
                rows: vec![
                    vec!["Item".into(), "Qty".into(), "Price".into()],
                    vec!["Widget".into(), "3".into(), "12.50".into()],
                    vec!["Gadget".into(), "1".into(), "99.00".into()],
                ],
            }],
        })
        .map_err(|e| e.to_string())?,
        written,
    )?;

    let long: Vec<String> = (1..=200)
        .map(|i| format!("Paragraph {i}: the quick brown fox jumps over the lazy dog."))
        .collect();
    let long_refs: Vec<&str> = long.iter().map(String::as_str).collect();
    save(
        "long.docx",
        Docx::from_paragraphs(&long_refs).map_err(|e| e.to_string())?,
        written,
    )?;

    let original = Docx::from_paragraphs(&[
        "The notice period is thirty (30) days.",
        "Payment is due on the Closing Date.",
        "This sentence stays the same.",
    ])
    .map_err(|e| e.to_string())?;
    let tracked = original
        .edit([EditOp::Replace {
            index: Some(1),
            old: "thirty (30)".into(),
            new: "sixty (60)".into(),
            content_match: None,
        }])
        .tracked("Alex Rivera")
        .granularity(Granularity::Word)
        .apply()
        .map_err(|e| e.to_string())?
        .document
        .ok_or_else(|| "tracked edit produced no document".to_string())?;
    save("tracked.docx", tracked, written)?;

    let commented = Docx::from_paragraphs(&["Please review the Closing Date in this paragraph."])
        .map_err(|e| e.to_string())?;
    let commented = commented
        .edit([EditOp::InsertComment {
            index: Some(1),
            content_match: None,
            anchor: Some("Closing Date".into()),
            text: "Confirm this date with the parties.".into(),
            author: Some("Alex Rivera".into()),
        }])
        .author("Alex Rivera")
        .apply()
        .map_err(|e| e.to_string())?
        .document
        .ok_or_else(|| "comment edit produced no document".to_string())?;
    save("comments.docx", commented, written)?;

    let revised = Docx::from_paragraphs(&[
        "The notice period is sixty (60) days.",
        "Payment is due on the Effective Date.",
        "This sentence stays the same.",
        "An extra closing paragraph.",
    ])
    .map_err(|e| e.to_string())?;
    let redlined = blackline_docx::redline(&original, &revised, "Alex Rivera", Granularity::Word)
        .map_err(|e| e.to_string())?;
    save("redline.docx", redlined, written)?;
    save("redline_original.docx", original, written)?;
    save("redline_revised.docx", revised, written)?;

    Ok(())
}

fn write_xlsx(dir: &Path, written: &mut Vec<String>) -> Result<(), String> {
    let save = |name: &str, wb: Xlsx, written: &mut Vec<String>| -> Result<(), String> {
        let path = dir.join(name);
        wb.save(&path).map_err(|e| e.to_string())?;
        written.push(path.display().to_string());
        Ok(())
    };

    save(
        "simple.xlsx",
        Xlsx::from_rows(
            "Sheet1",
            &[
                vec!["Name", "Role"],
                vec!["Ada Lovelace", "Engineer"],
                vec!["Alan Turing", "Mathematician"],
            ],
        )
        .map_err(|e| e.to_string())?,
        written,
    )?;

    save(
        "empty.xlsx",
        Xlsx::create(&XlsxSpec { sheets: Vec::new() }).map_err(|e| e.to_string())?,
        written,
    )?;

    let mut cells = serde_json::Map::new();
    cells.insert("A1".into(), serde_json::json!("Total"));
    cells.insert("B1".into(), serde_json::json!(10));
    cells.insert("B2".into(), serde_json::json!(20));
    cells.insert("B3".into(), serde_json::json!("=SUM(B1:B2)"));
    cells.insert("C1".into(), serde_json::json!(true));
    save(
        "formulas.xlsx",
        Xlsx::create(&XlsxSpec {
            sheets: vec![SheetSpec {
                name: "Calc".into(),
                cells,
                rows: Vec::new(),
            }],
        })
        .map_err(|e| e.to_string())?,
        written,
    )?;

    save(
        "multi_sheet.xlsx",
        Xlsx::create(&XlsxSpec {
            sheets: vec![
                SheetSpec {
                    name: "North".into(),
                    cells: Default::default(),
                    rows: vec![
                        vec![serde_json::json!("Q1"), serde_json::json!(100)],
                        vec![serde_json::json!("Q2"), serde_json::json!(150)],
                    ],
                },
                SheetSpec {
                    name: "South".into(),
                    cells: Default::default(),
                    rows: vec![
                        vec![serde_json::json!("Q1"), serde_json::json!(80)],
                        vec![serde_json::json!("Q2"), serde_json::json!(90)],
                    ],
                },
            ],
        })
        .map_err(|e| e.to_string())?,
        written,
    )?;

    let mut large_rows = Vec::new();
    large_rows.push(
        (0..20)
            .map(|c| serde_json::Value::String(format!("Col{c}")))
            .collect(),
    );
    for r in 1..50 {
        large_rows.push((0..20).map(|c| serde_json::json!(r * 20 + c)).collect());
    }
    save(
        "large.xlsx",
        Xlsx::create(&XlsxSpec {
            sheets: vec![SheetSpec {
                name: "Grid".into(),
                cells: Default::default(),
                rows: large_rows,
            }],
        })
        .map_err(|e| e.to_string())?,
        written,
    )?;

    Ok(())
}

fn write_pptx(dir: &Path, written: &mut Vec<String>) -> Result<(), String> {
    let save = |name: &str, p: Pptx, written: &mut Vec<String>| -> Result<(), String> {
        let path = dir.join(name);
        p.save(&path).map_err(|e| e.to_string())?;
        written.push(path.display().to_string());
        Ok(())
    };

    save(
        "simple.pptx",
        Pptx::create(&PptxSpec {
            slides: vec![SlideSpec {
                texts: vec!["Quarterly Review".into(), "Highlights".into()],
                notes: None,
            }],
        })
        .map_err(|e| e.to_string())?,
        written,
    )?;

    save(
        "multi_slide.pptx",
        Pptx::create(&PptxSpec {
            slides: vec![
                SlideSpec {
                    texts: vec!["Agenda".into(), "Three topics".into()],
                    notes: Some("Keep this short.".into()),
                },
                SlideSpec {
                    texts: vec!["Topic A".into(), "Details for A".into()],
                    notes: None,
                },
                SlideSpec {
                    texts: vec!["Topic B".into(), "Details for B".into()],
                    notes: None,
                },
            ],
        })
        .map_err(|e| e.to_string())?,
        written,
    )?;

    let slides: Vec<SlideSpec> = (1..=20)
        .map(|i| SlideSpec {
            texts: vec![format!("Slide {i}"), format!("Body copy for slide {i}.")],
            notes: None,
        })
        .collect();
    save(
        "long_deck.pptx",
        Pptx::create(&PptxSpec { slides }).map_err(|e| e.to_string())?,
        written,
    )?;

    Ok(())
}

fn para(text: &str) -> ParaSpec {
    ParaSpec {
        text: Some(text.into()),
        style: None,
        runs: None,
        props: ParaProps::default(),
    }
}

fn para_styled(text: &str, style: &str) -> ParaSpec {
    ParaSpec {
        text: Some(text.into()),
        style: Some(style.into()),
        runs: None,
        props: ParaProps::default(),
    }
}

fn run_spec(text: &str, props: Option<RunProps>) -> RunSpec {
    RunSpec {
        text: text.into(),
        props: props.unwrap_or_default(),
    }
}
