//! Package + TreeOp + validate integration tests.

use blackline_core::package::Package;
use blackline_core::tree::{apply_ops, NodePath, TreeOp};
use blackline_core::validate;
use blackline_core::xml::{self, XmlNode};
use blackline_core::{ContentTypes, Relationship, Relationships};

fn minimal_docx() -> Package {
    let mut pkg = Package::new();
    let mut ct = ContentTypes::office_defaults();
    ct.ensure_override(
        "word/document.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
    );
    pkg.set_content_types(&ct);

    let mut rels = Relationships::default();
    rels.add(Relationship::internal(
        "rId1",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
        "word/document.xml",
    ));
    pkg.set_rels_for("", &rels);

    let doc = blackline_core::XmlDocument::new(
        XmlNode::w("document").with_child(
            XmlNode::w("body").with_child(
                XmlNode::w("p")
                    .with_child(XmlNode::w("r").with_child(XmlNode::w("t").with_text("hello"))),
            ),
        ),
    );
    pkg.set_part_xml("word/document.xml", &doc);
    pkg
}

#[test]
fn validate_passes_on_minimal_package() {
    let pkg = minimal_docx();
    let report = validate::check(&pkg).unwrap();
    assert!(report.passed(), "{report:?}");
}

#[test]
fn main_document_part_resolves() {
    let pkg = minimal_docx();
    assert_eq!(pkg.main_document_part().unwrap(), "word/document.xml");
}

#[test]
fn tree_op_json_roundtrip_and_apply() {
    let raw = r#"{"action":"set_text","path":"body/p[0]/r[0]/t","text":"changed"}"#;
    let op: TreeOp = serde_json::from_str(raw).unwrap();
    let mut doc = xml::parse(
        br#"<w:document><w:body><w:p><w:r><w:t>hello</w:t></w:r></w:p></w:body></w:document>"#,
    )
    .unwrap();
    apply_ops(&mut doc.root, &[op], true).unwrap();
    let node = NodePath::parse("body/p[0]/r[0]/t")
        .unwrap()
        .resolve(&doc.root)
        .unwrap();
    assert_eq!(node.text_content(), "changed");
}

#[test]
fn set_attr_replace_and_remove_child() {
    let mut root = xml::parse(b"<root><p id=\"1\">a</p><p>b</p></root>")
        .unwrap()
        .root;
    apply_ops(
        &mut root,
        &[
            TreeOp::SetAttr {
                path: "p[0]".into(),
                name: "id".into(),
                value: "9".into(),
            },
            TreeOp::Replace {
                path: "p[1]".into(),
                xml: "<p>c</p>".into(),
            },
            TreeOp::RemoveChild {
                path: "".into(),
                index: 0,
            },
        ],
        true,
    )
    .unwrap();
    assert_eq!(root.find_all("p").len(), 1);
    assert_eq!(root.child_named("p", 0).unwrap().text_content(), "c");
}

#[test]
fn remove_attr_and_insert_child() {
    let mut root = xml::parse(b"<root><p id=\"1\">a</p></root>").unwrap().root;
    apply_ops(
        &mut root,
        &[
            TreeOp::RemoveAttr {
                path: "p[0]".into(),
                name: "id".into(),
            },
            TreeOp::InsertChild {
                path: "".into(),
                index: None,
                xml: "<p>b</p>".into(),
            },
        ],
        true,
    )
    .unwrap();
    assert!(root.child_named("p", 0).unwrap().get_attr("id").is_none());
    assert_eq!(root.find_all("p").len(), 2);
}

#[test]
fn unpack_pack_preserves_text() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("in.docx");
    let unpacked = dir.path().join("unpacked");
    let packed = dir.path().join("out.docx");
    minimal_docx().save(&src).unwrap();
    blackline_core::opc::unpack_archive(&src, &unpacked).unwrap();
    assert!(unpacked.join("word/document.xml").exists());
    blackline_core::opc::pack_dir(&unpacked, &packed).unwrap();
    let again = Package::open(&packed).unwrap();
    let xml = again.part_xml("word/document.xml").unwrap();
    assert_eq!(xml.root.text_content(), "hello");
    assert!(validate::check(&again).unwrap().passed());
}

#[test]
fn missing_part_is_an_error() {
    let pkg = Package::new();
    assert!(pkg.part("nope.xml").is_err());
}

#[test]
fn content_types_override_roundtrip() {
    let mut ct = ContentTypes::office_defaults();
    ct.ensure_override("word/document.xml", "application/xml");
    ct.ensure_override("word/document.xml", "application/xml");
    assert_eq!(ct.overrides.len(), 1);
    ct.remove_override("word/document.xml");
    assert!(ct.overrides.is_empty());
}

#[test]
fn strict_tree_op_stops_on_bad_path() {
    let mut root = xml::parse(b"<root><p>a</p></root>").unwrap().root;
    let err = apply_ops(
        &mut root,
        &[TreeOp::SetText {
            path: "missing[0]".into(),
            text: "x".into(),
        }],
        true,
    );
    assert!(err.is_err());
    assert_eq!(root.text_content(), "a");
}
