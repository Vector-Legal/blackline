//! Build a minimal Word-openable package from a JSON spec.

use serde::Deserialize;

use blackline_core::ns;
use blackline_core::package::Package;
use blackline_core::rels::{Relationship, Relationships};
use blackline_core::time::utc_now_iso;
use blackline_core::xml::{XmlDocument, XmlNode};

use crate::error::DocxError;
use crate::style::{build_paragraph, build_run, ParaProps, RunProps};

const THEME_XML: &[u8] = include_bytes!("assets/theme1.xml");
const STYLES_WITH_EFFECTS_XML: &[u8] = include_bytes!("assets/stylesWithEffects.xml");

/// A document specification.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSpec {
    /// Optional default font.
    #[serde(default)]
    pub font: Option<String>,
    /// Body blocks.
    #[serde(default)]
    pub paragraphs: Vec<ParaSpec>,
    /// Alternative: sections containing paragraphs (docx-js style).
    #[serde(default)]
    pub sections: Vec<SectionSpec>,
    /// Tables appended after paragraphs.
    #[serde(default)]
    pub tables: Vec<TableSpec>,
}

/// A table in a create spec.
#[derive(Debug, Clone, Deserialize)]
pub struct TableSpec {
    /// Row-major cell texts.
    pub rows: Vec<Vec<String>>,
}

/// One section of a create spec.
#[derive(Debug, Clone, Deserialize)]
pub struct SectionSpec {
    /// Paragraphs in this section.
    #[serde(default)]
    pub children: Vec<ParaSpec>,
    /// Alias used by some specs.
    #[serde(default)]
    pub paragraphs: Vec<ParaSpec>,
}

/// One paragraph in a create spec.
#[derive(Debug, Clone, Deserialize)]
pub struct ParaSpec {
    /// Plain text (single run).
    #[serde(default)]
    pub text: Option<String>,
    /// Style id.
    #[serde(default)]
    pub style: Option<String>,
    /// Mixed runs.
    #[serde(default)]
    pub runs: Option<Vec<RunSpec>>,
    /// Paragraph properties.
    #[serde(flatten)]
    pub props: ParaProps,
}

/// One run in a create spec.
#[derive(Debug, Clone, Deserialize)]
pub struct RunSpec {
    /// Run text.
    pub text: String,
    /// Run properties.
    #[serde(flatten)]
    pub props: RunProps,
}

/// Build a package from `spec`.
pub fn create(spec: &CreateSpec) -> Result<Package, DocxError> {
    let mut paras: Vec<ParaSpec> = spec.paragraphs.clone();
    for section in &spec.sections {
        paras.extend(section.children.iter().cloned());
        paras.extend(section.paragraphs.iter().cloned());
    }
    if paras.is_empty() {
        paras.push(ParaSpec {
            text: Some(String::new()),
            style: None,
            runs: None,
            props: ParaProps::default(),
        });
    }

    let mut body = XmlNode::w("body");
    for p in &paras {
        body = body.with_child(para_from_spec(p));
    }
    for table in &spec.tables {
        body = body.with_child(table_from_spec(table));
    }
    body = body.with_child(default_sect_pr());

    let document = XmlDocument::new(
        XmlNode::w("document")
            .with_attr("xmlns:w", ns::W)
            .with_attr("xmlns:r", ns::R)
            .with_attr("xmlns:mc", ns::MC)
            .with_attr("mc:Ignorable", "w14")
            .with_child(body),
    );

    let mut pkg = Package::new();
    let mut ct = blackline_core::ContentTypes::office_defaults();
    ct.ensure_override("word/document.xml", ns::content::DOCUMENT);
    ct.ensure_override("word/styles.xml", ns::content::STYLES);
    ct.ensure_override("word/settings.xml", ns::content::SETTINGS);
    ct.ensure_override("word/theme/theme1.xml", ns::content::THEME);
    ct.ensure_override(
        "word/stylesWithEffects.xml",
        "application/vnd.ms-word.stylesWithEffects+xml",
    );
    ct.ensure_override("docProps/core.xml", ns::content::CORE_PROPS);
    ct.ensure_override("docProps/app.xml", ns::content::APP_PROPS);
    pkg.set_content_types(&ct);

    let mut pkg_rels = Relationships::default();
    pkg_rels.add(Relationship::internal(
        "rId1",
        ns::rel::OFFICE_DOCUMENT,
        "word/document.xml",
    ));
    pkg_rels.add(Relationship::internal(
        "rId2",
        ns::rel::CORE_PROPERTIES,
        "docProps/core.xml",
    ));
    pkg_rels.add(Relationship::internal(
        "rId3",
        ns::rel::EXTENDED_PROPERTIES,
        "docProps/app.xml",
    ));
    pkg.set_rels_for("", &pkg_rels);

    let mut doc_rels = Relationships::default();
    doc_rels.add(Relationship::internal(
        "rId1",
        ns::rel::STYLES,
        "styles.xml",
    ));
    doc_rels.add(Relationship::internal(
        "rId2",
        ns::rel::SETTINGS,
        "settings.xml",
    ));
    doc_rels.add(Relationship::internal(
        "rId3",
        ns::rel::THEME,
        "theme/theme1.xml",
    ));
    doc_rels.add(Relationship::internal(
        "rId4",
        "http://schemas.microsoft.com/office/2007/relationships/stylesWithEffects",
        "stylesWithEffects.xml",
    ));
    pkg.set_rels_for("word/document.xml", &doc_rels);

    pkg.set_part_xml("word/document.xml", &document);
    pkg.set_part("word/styles.xml", styles_xml(spec.font.as_deref()));
    pkg.set_part("word/settings.xml", settings_xml());
    pkg.set_part("word/theme/theme1.xml", THEME_XML);
    pkg.set_part("word/stylesWithEffects.xml", STYLES_WITH_EFFECTS_XML);
    pkg.set_part("docProps/core.xml", core_xml());
    pkg.set_part("docProps/app.xml", app_xml());
    Ok(pkg)
}

fn para_from_spec(p: &ParaSpec) -> XmlNode {
    if let Some(runs) = &p.runs {
        let mut para = XmlNode::w("p");
        if let Some(style) = &p.style {
            para = para.with_child(
                XmlNode::w("pPr").with_child(XmlNode::w("pStyle").with_attr("w:val", style)),
            );
        }
        for r in runs {
            para = para.with_child(build_run(&r.text, Some(&r.props)));
        }
        p.props.apply_to_para(&mut para);
        para
    } else {
        let mut para = build_paragraph(p.text.as_deref().unwrap_or(""), p.style.as_deref());
        p.props.apply_to_para(&mut para);
        para
    }
}

fn table_from_spec(table: &TableSpec) -> XmlNode {
    let mut tbl = XmlNode::w("tbl").with_child(
        XmlNode::w("tblPr").with_child(
            XmlNode::w("tblW")
                .with_attr("w:w", "0")
                .with_attr("w:type", "auto"),
        ),
    );
    for row in &table.rows {
        let mut tr = XmlNode::w("tr");
        for cell in row {
            tr = tr.with_child(XmlNode::w("tc").with_child(build_paragraph(cell, None)));
        }
        tbl = tbl.with_child(tr);
    }
    tbl
}

fn default_sect_pr() -> XmlNode {
    XmlNode::w("sectPr")
        .with_child(
            XmlNode::w("pgSz")
                .with_attr("w:w", "12240")
                .with_attr("w:h", "15840"),
        )
        .with_child(
            XmlNode::w("pgMar")
                .with_attr("w:top", "1440")
                .with_attr("w:right", "1440")
                .with_attr("w:bottom", "1440")
                .with_attr("w:left", "1440")
                .with_attr("w:header", "720")
                .with_attr("w:footer", "720"),
        )
}

fn styles_xml(default_font: Option<&str>) -> Vec<u8> {
    let font = default_font.unwrap_or("Calibri");
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="{w}">
  <w:docDefaults>
    <w:rPrDefault><w:rPr>
      <w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:eastAsia="{font}" w:cs="{font}"/>
      <w:sz w:val="22"/><w:szCs w:val="22"/>
    </w:rPr></w:rPrDefault>
    <w:pPrDefault><w:pPr/></w:pPrDefault>
  </w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal">
    <w:name w:val="Normal"/><w:qFormat/>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:qFormat/>
    <w:pPr><w:outlineLvl w:val="0"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="32"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading2">
    <w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:qFormat/>
    <w:pPr><w:outlineLvl w:val="1"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="26"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading3">
    <w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:qFormat/>
    <w:pPr><w:outlineLvl w:val="2"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="24"/></w:rPr>
  </w:style>
</w:styles>"#,
        w = ns::W,
    );
    xml.into_bytes()
}

fn settings_xml() -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="{w}">
  <w:zoom w:percent="100"/>
  <w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>
</w:settings>"#,
        w = ns::W,
    )
    .into_bytes()
}

fn core_xml() -> Vec<u8> {
    let now = utc_now_iso();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="{cp}" xmlns:dc="{dc}" xmlns:dcterms="{dcterms}" xmlns:xsi="{xsi}">
  <dc:creator>blackline</dc:creator>
  <cp:lastModifiedBy>blackline</cp:lastModifiedBy>
  <dcterms:created xsi:type="dcterms:W3CDTF">{now}</dcterms:created>
  <dcterms:modified xsi:type="dcterms:W3CDTF">{now}</dcterms:modified>
</cp:coreProperties>"#,
        cp = ns::CP,
        dc = ns::DC,
        dcterms = ns::DCTERMS,
        xsi = ns::XSI,
    )
    .into_bytes()
}

fn app_xml() -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="{ep}"><Application>blackline</Application></Properties>"#,
        ep = ns::EP,
    )
    .into_bytes()
}

/// Convenience: a document with one paragraph per string.
pub fn from_paragraphs(texts: &[&str]) -> Result<Package, DocxError> {
    let spec = CreateSpec {
        font: None,
        paragraphs: texts
            .iter()
            .map(|t| ParaSpec {
                text: Some((*t).to_string()),
                style: None,
                runs: None,
                props: ParaProps::default(),
            })
            .collect(),
        sections: Vec::new(),
        tables: Vec::new(),
    };
    create(&spec)
}
