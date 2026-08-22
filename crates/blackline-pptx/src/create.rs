//! Build a minimal PowerPoint-openable presentation.

use serde::Deserialize;

use blackline_core::ns;
use blackline_core::package::Package;
use blackline_core::rels::{Relationship, Relationships};
use blackline_core::time::utc_now_iso;
use blackline_core::xml::{XmlDocument, XmlNode};

use crate::error::PptxError;

/// Presentation spec.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSpec {
    /// Slides.
    #[serde(default)]
    pub slides: Vec<SlideSpec>,
}

/// One slide.
#[derive(Debug, Clone, Deserialize)]
pub struct SlideSpec {
    /// Text boxes, in order.
    #[serde(default)]
    pub texts: Vec<String>,
    /// Speaker notes.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Build a package.
pub fn create(spec: &CreateSpec) -> Result<Package, PptxError> {
    let slides = if spec.slides.is_empty() {
        vec![SlideSpec {
            texts: vec!["Title".into()],
            notes: None,
        }]
    } else {
        spec.slides.clone()
    };

    let mut pkg = Package::new();
    let mut ct = blackline_core::ContentTypes::office_defaults();
    ct.ensure_override("ppt/presentation.xml", ns::content::PRESENTATION);
    ct.ensure_override(
        "ppt/slideMasters/slideMaster1.xml",
        ns::content::SLIDE_MASTER,
    );
    ct.ensure_override(
        "ppt/slideLayouts/slideLayout1.xml",
        ns::content::SLIDE_LAYOUT,
    );
    ct.ensure_override("ppt/theme/theme1.xml", ns::content::THEME);
    ct.ensure_override("docProps/core.xml", ns::content::CORE_PROPS);
    ct.ensure_override("docProps/app.xml", ns::content::APP_PROPS);

    let mut pkg_rels = Relationships::default();
    pkg_rels.add(Relationship::internal(
        "rId1",
        ns::rel::OFFICE_DOCUMENT,
        "ppt/presentation.xml",
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

    pkg.set_part("ppt/theme/theme1.xml", theme_xml());
    pkg.set_part_xml("ppt/slideMasters/slideMaster1.xml", &slide_master());
    pkg.set_part_xml("ppt/slideLayouts/slideLayout1.xml", &slide_layout());

    let mut master_rels = Relationships::default();
    master_rels.add(Relationship::internal(
        "rId1",
        ns::rel::SLIDE_LAYOUT,
        "../slideLayouts/slideLayout1.xml",
    ));
    master_rels.add(Relationship::internal(
        "rId2",
        ns::rel::THEME,
        "../theme/theme1.xml",
    ));
    pkg.set_rels_for("ppt/slideMasters/slideMaster1.xml", &master_rels);

    let mut layout_rels = Relationships::default();
    layout_rels.add(Relationship::internal(
        "rId1",
        ns::rel::SLIDE_MASTER,
        "../slideMasters/slideMaster1.xml",
    ));
    pkg.set_rels_for("ppt/slideLayouts/slideLayout1.xml", &layout_rels);

    let mut pres_rels = Relationships::default();
    pres_rels.add(Relationship::internal(
        "rId1",
        ns::rel::SLIDE_MASTER,
        "slideMasters/slideMaster1.xml",
    ));

    let mut sldidlst = XmlNode::p("sldIdLst");
    for (i, slide) in slides.iter().enumerate() {
        let n = i + 1;
        let part = format!("ppt/slides/slide{n}.xml");
        ct.ensure_override(&part, ns::content::SLIDE);
        let rid = format!("rId{}", n + 1);
        pres_rels.add(Relationship::internal(
            &rid,
            ns::rel::SLIDE,
            format!("slides/slide{n}.xml"),
        ));
        sldidlst = sldidlst.with_child(
            XmlNode::p("sldId")
                .with_attr("id", (256 + i as u32).to_string())
                .with_attr("r:id", &rid),
        );
        pkg.set_part_xml(&part, &slide_xml(slide, n as u32));
        let mut slide_rels = Relationships::default();
        slide_rels.add(Relationship::internal(
            "rId1",
            ns::rel::SLIDE_LAYOUT,
            "../slideLayouts/slideLayout1.xml",
        ));
        pkg.set_rels_for(&part, &slide_rels);
    }
    pkg.set_rels_for("ppt/presentation.xml", &pres_rels);

    let pres = XmlDocument::new(
        XmlNode::p("presentation")
            .with_attr("xmlns:a", ns::A)
            .with_attr("xmlns:r", ns::R)
            .with_attr("xmlns:p", ns::P)
            .with_child(
                XmlNode::p("sldMasterIdLst").with_child(
                    XmlNode::p("sldMasterId")
                        .with_attr("id", "2147483648")
                        .with_attr("r:id", "rId1"),
                ),
            )
            .with_child(sldidlst)
            .with_child(
                XmlNode::p("sldSz")
                    .with_attr("cx", "9144000")
                    .with_attr("cy", "6858000"),
            ),
    );
    pkg.set_part_xml("ppt/presentation.xml", &pres);
    pkg.set_part("docProps/core.xml", core_xml());
    pkg.set_part("docProps/app.xml", app_xml());
    pkg.set_content_types(&ct);
    Ok(pkg)
}

fn slide_xml(spec: &SlideSpec, start_id: u32) -> XmlDocument {
    let mut sp_tree = XmlNode::p("spTree")
        .with_child(
            XmlNode::p("nvGrpSpPr")
                .with_child(
                    XmlNode::p("cNvPr")
                        .with_attr("id", "1")
                        .with_attr("name", ""),
                )
                .with_child(XmlNode::p("cNvGrpSpPr"))
                .with_child(XmlNode::p("nvPr")),
        )
        .with_child(XmlNode::p("grpSpPr"));
    for (i, text) in spec.texts.iter().enumerate() {
        sp_tree = sp_tree.with_child(text_box(start_id * 10 + i as u32 + 2, text, i));
    }
    XmlDocument::new(
        XmlNode::p("sld")
            .with_attr("xmlns:a", ns::A)
            .with_attr("xmlns:r", ns::R)
            .with_attr("xmlns:p", ns::P)
            .with_child(XmlNode::p("cSld").with_child(sp_tree)),
    )
}

fn text_box(id: u32, text: &str, index: usize) -> XmlNode {
    let y = 400000 + index as i64 * 800000;
    XmlNode::p("sp")
        .with_child(
            XmlNode::p("nvSpPr")
                .with_child(
                    XmlNode::p("cNvPr")
                        .with_attr("id", id.to_string())
                        .with_attr("name", format!("Text {id}")),
                )
                .with_child(
                    XmlNode::p("cNvSpPr").with_child(XmlNode::a("spLocks").with_attr("noGrp", "1")),
                )
                .with_child(XmlNode::p("nvPr")),
        )
        .with_child(
            XmlNode::p("spPr").with_child(
                XmlNode::a("xfrm")
                    .with_child(
                        XmlNode::a("off")
                            .with_attr("x", "457200")
                            .with_attr("y", y.to_string()),
                    )
                    .with_child(
                        XmlNode::a("ext")
                            .with_attr("cx", "8229600")
                            .with_attr("cy", "685800"),
                    ),
            ),
        )
        .with_child(
            XmlNode::p("txBody")
                .with_child(XmlNode::a("bodyPr"))
                .with_child(XmlNode::a("lstStyle"))
                .with_child(
                    XmlNode::a("p")
                        .with_child(XmlNode::a("r").with_child(XmlNode::a("t").with_text(text))),
                ),
        )
}

fn slide_master() -> XmlDocument {
    XmlDocument::new(
        XmlNode::p("sldMaster")
            .with_attr("xmlns:a", ns::A)
            .with_attr("xmlns:r", ns::R)
            .with_attr("xmlns:p", ns::P)
            .with_child(
                XmlNode::p("cSld").with_child(
                    XmlNode::p("spTree")
                        .with_child(
                            XmlNode::p("nvGrpSpPr")
                                .with_child(
                                    XmlNode::p("cNvPr")
                                        .with_attr("id", "1")
                                        .with_attr("name", ""),
                                )
                                .with_child(XmlNode::p("cNvGrpSpPr"))
                                .with_child(XmlNode::p("nvPr")),
                        )
                        .with_child(XmlNode::p("grpSpPr")),
                ),
            )
            .with_child(
                XmlNode::p("sldLayoutIdLst").with_child(
                    XmlNode::p("sldLayoutId")
                        .with_attr("id", "2147483649")
                        .with_attr("r:id", "rId1"),
                ),
            ),
    )
}

fn slide_layout() -> XmlDocument {
    XmlDocument::new(
        XmlNode::p("sldLayout")
            .with_attr("xmlns:a", ns::A)
            .with_attr("xmlns:r", ns::R)
            .with_attr("xmlns:p", ns::P)
            .with_child(
                XmlNode::p("cSld").with_child(
                    XmlNode::p("spTree")
                        .with_child(
                            XmlNode::p("nvGrpSpPr")
                                .with_child(
                                    XmlNode::p("cNvPr")
                                        .with_attr("id", "1")
                                        .with_attr("name", ""),
                                )
                                .with_child(XmlNode::p("cNvGrpSpPr"))
                                .with_child(XmlNode::p("nvPr")),
                        )
                        .with_child(XmlNode::p("grpSpPr")),
                ),
            ),
    )
}

fn theme_xml() -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="{a}" name="blackline">
  <a:themeElements>
    <a:clrScheme name="Office">
      <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
      <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
      <a:dk2><a:srgbClr val="1F497D"/></a:dk2>
      <a:lt2><a:srgbClr val="EEECE1"/></a:lt2>
      <a:accent1><a:srgbClr val="4F81BD"/></a:accent1>
      <a:accent2><a:srgbClr val="C0504D"/></a:accent2>
      <a:accent3><a:srgbClr val="9BBB59"/></a:accent3>
      <a:accent4><a:srgbClr val="8064A2"/></a:accent4>
      <a:accent5><a:srgbClr val="4BACC6"/></a:accent5>
      <a:accent6><a:srgbClr val="F79646"/></a:accent6>
      <a:hlink><a:srgbClr val="0000FF"/></a:hlink>
      <a:folHlink><a:srgbClr val="800080"/></a:folHlink>
    </a:clrScheme>
    <a:fontScheme name="Office">
      <a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>
      <a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont>
    </a:fontScheme>
    <a:fmtScheme name="Office">
      <a:fillStyleLst><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:fillStyleLst>
      <a:lnStyleLst><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></a:lnStyleLst>
      <a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>
      <a:bgFillStyleLst><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:bgFillStyleLst>
    </a:fmtScheme>
  </a:themeElements>
</a:theme>"#,
        a = ns::A,
    )
    .into_bytes()
}

fn core_xml() -> Vec<u8> {
    let now = utc_now_iso();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="{cp}" xmlns:dc="{dc}" xmlns:dcterms="{dcterms}" xmlns:xsi="{xsi}">
  <dc:creator>blackline</dc:creator>
  <dcterms:created xsi:type="dcterms:W3CDTF">{now}</dcterms:created>
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
