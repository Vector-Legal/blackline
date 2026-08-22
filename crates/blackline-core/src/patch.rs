//! RFC 5261 XML Patch — compile `add` / `replace` / `remove` to [`TreeOp`].
//!
//! This is a **mutation** module. It does not invent a new edit algebra:
//! every operation is a selector plus one of the three IETF verbs, and
//! those verbs compile to [`crate::tree::TreeOp`]. Apply is sequential
//! (each op sees the tree left by the previous one), matching RFC 5261.
//!
//! [RFC 5261](https://www.rfc-editor.org/rfc/rfc5261.html) requires a
//! unique target. Zero or many matches is an error.
//!
//! JSON (agent-first) and the RFC's XML `<diff>` document are both
//! accepted. Namespace-axis patches (`type="namespace::…"`) are out of
//! scope — this crate matches local names, like [`crate::NodePath`].

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::formula::{self, Match};
use crate::tree::{self, TreeOp, TreeOpReport};
use crate::xml::XmlNode;

/// Where an `<add>` places its payload relative to `sel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PatchPos {
    /// Last child of the selected element (RFC 5261 default).
    #[default]
    Append,
    /// First child of the selected element.
    Prepend,
    /// Immediate preceding sibling.
    Before,
    /// Immediate following sibling.
    After,
}

/// One RFC 5261 operation, in the JSON form the CLI prefers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum PatchOp {
    /// Add a child, sibling, or attribute.
    Add {
        /// RFC 5261 `sel`.
        sel: String,
        /// `pos`. Default [`PatchPos::Append`].
        #[serde(default)]
        pos: PatchPos,
        /// RFC 5261 `type`. `"@name"` adds an attribute; omit for nodes.
        #[serde(default, rename = "type")]
        type_attr: Option<String>,
        /// Element fragment(s).
        #[serde(default)]
        xml: Option<String>,
        /// Text payload (attribute value, or a text node).
        #[serde(default)]
        text: Option<String>,
    },
    /// Replace the unique target node or attribute.
    Replace {
        /// RFC 5261 `sel`.
        sel: String,
        /// Replacement element.
        #[serde(default)]
        xml: Option<String>,
        /// Replacement text / attribute value.
        #[serde(default)]
        text: Option<String>,
    },
    /// Remove the unique target node or attribute.
    Remove {
        /// RFC 5261 `sel`.
        sel: String,
    },
}

/// Parse `--ops` as a JSON array or an RFC 5261 `<diff>` document.
pub fn parse_ops(raw: &str) -> Result<Vec<PatchOp>, CoreError> {
    let trimmed = raw.trim();
    if trimmed.starts_with('<') {
        parse_rfc_xml(trimmed)
    } else {
        serde_json::from_str(trimmed)
            .map_err(|e| CoreError::patch(format!("invalid patch JSON: {e}")))
    }
}

/// Parse `<diff><add sel="…">…</add>…</diff>` (root name is ignored).
pub fn parse_rfc_xml(xml: &str) -> Result<Vec<PatchOp>, CoreError> {
    let doc = crate::xml::parse(xml.as_bytes()).map_err(|e| CoreError::patch(e.to_string()))?;
    let mut ops = Vec::new();
    for child in doc.root.children() {
        if !child.is_element() {
            continue;
        }
        let sel = child
            .get_attr("sel")
            .ok_or_else(|| CoreError::patch(format!("<{}> missing sel", child.local_name())))?
            .to_string();
        match child.local_name() {
            "add" => {
                let pos = match child.get_attr("pos") {
                    Some("prepend") => PatchPos::Prepend,
                    Some("before") => PatchPos::Before,
                    Some("after") => PatchPos::After,
                    Some("append") | None => PatchPos::Append,
                    Some(other) => {
                        return Err(CoreError::patch(format!("unknown pos '{other}'")));
                    }
                };
                let type_attr = child.get_attr("type").map(str::to_string);
                let (xml, text) = payload_from_node(child);
                ops.push(PatchOp::Add {
                    sel,
                    pos,
                    type_attr,
                    xml,
                    text,
                });
            }
            "replace" => {
                let (xml, text) = payload_from_node(child);
                ops.push(PatchOp::Replace { sel, xml, text });
            }
            "remove" => ops.push(PatchOp::Remove { sel }),
            other => {
                return Err(CoreError::patch(format!(
                    "unknown patch element <{other}>; expected add, replace, or remove"
                )));
            }
        }
    }
    Ok(ops)
}

fn payload_from_node(node: &XmlNode) -> (Option<String>, Option<String>) {
    let elements: Vec<&XmlNode> = node.children().iter().filter(|c| c.is_element()).collect();
    if !elements.is_empty() {
        let xml = elements
            .iter()
            .map(|e| e.to_xml_string())
            .collect::<Vec<_>>()
            .join("");
        return (Some(xml), None);
    }
    let text = node.text_content();
    if text.is_empty() {
        (None, None)
    } else {
        (None, Some(text))
    }
}

/// Compile and apply `ops` left to right (RFC 5261 sequential semantics).
pub fn apply(
    root: &mut XmlNode,
    ops: &[PatchOp],
    strict: bool,
) -> Result<Vec<TreeOpReport>, CoreError> {
    let planned = plan(root, ops)?;
    tree::apply_ops(root, &planned, strict)
}

/// Compile `ops` against a clone so later selectors see earlier edits.
/// The returned [`TreeOp`]s are valid when applied in order to the original.
pub fn plan(root: &XmlNode, ops: &[PatchOp]) -> Result<Vec<TreeOp>, CoreError> {
    let mut clone = root.clone();
    let mut all = Vec::new();
    for op in ops {
        let compiled = compile_one(&clone, op)?;
        tree::apply_ops(&mut clone, &compiled, true)?;
        all.extend(compiled);
    }
    Ok(all)
}

fn compile_one(root: &XmlNode, op: &PatchOp) -> Result<Vec<TreeOp>, CoreError> {
    match op {
        PatchOp::Add {
            sel,
            pos,
            type_attr,
            xml,
            text,
        } => compile_add(
            root,
            sel,
            *pos,
            type_attr.as_deref(),
            xml.as_deref(),
            text.as_deref(),
        ),
        PatchOp::Replace { sel, xml, text } => {
            compile_replace(root, sel, xml.as_deref(), text.as_deref())
        }
        PatchOp::Remove { sel } => compile_remove(root, sel),
    }
}

fn unique_match(root: &XmlNode, sel: &str) -> Result<Match, CoreError> {
    let matches = formula::select_str(root, sel)?;
    match matches.len() {
        1 => Ok(matches.into_iter().next().expect("len 1")),
        0 => Err(CoreError::patch(format!("sel '{sel}' matched nothing"))),
        n => Err(CoreError::patch(format!(
            "sel '{sel}' matched {n} nodes; RFC 5261 requires exactly one"
        ))),
    }
}

fn compile_add(
    root: &XmlNode,
    sel: &str,
    pos: PatchPos,
    type_attr: Option<&str>,
    xml: Option<&str>,
    text: Option<&str>,
) -> Result<Vec<TreeOp>, CoreError> {
    let target = unique_match(root, sel)?;
    if let Some(ty) = type_attr {
        if let Some(name) = ty.strip_prefix('@') {
            let value = text.unwrap_or("").to_string();
            return Ok(vec![TreeOp::SetAttr {
                path: target.node_path()?.to_op_path(),
                name: name.to_string(),
                value,
            }]);
        }
        if ty.starts_with("namespace:") || ty.starts_with("namespace::") {
            return Err(CoreError::patch(
                "namespace-axis add is not supported; this crate matches local names",
            ));
        }
        return Err(CoreError::patch(format!(
            "unsupported add type '{ty}'; use @attr for attributes"
        )));
    }
    if target.attr.is_some() {
        return Err(CoreError::patch(
            "add with an attribute sel needs type=\"@name\" on the parent, or use replace",
        ));
    }
    let payload = payload_xml(xml, text)?;
    insert_ops(root, &target, pos, &payload)
}

fn compile_replace(
    root: &XmlNode,
    sel: &str,
    xml: Option<&str>,
    text: Option<&str>,
) -> Result<Vec<TreeOp>, CoreError> {
    let target = unique_match(root, sel)?;
    if let Some(attr) = target.attr.as_deref() {
        let value = text
            .map(str::to_string)
            .or_else(|| xml.map(str::to_string))
            .unwrap_or_default();
        return Ok(vec![TreeOp::SetAttr {
            path: target.node_path()?.to_op_path(),
            name: attr.to_string(),
            value,
        }]);
    }
    if let Some(fragment) = xml {
        return Ok(vec![TreeOp::Replace {
            path: target.node_path()?.to_op_path(),
            xml: fragment.to_string(),
        }]);
    }
    Ok(vec![TreeOp::SetText {
        path: target.node_path()?.to_op_path(),
        text: text.unwrap_or("").to_string(),
    }])
}

fn compile_remove(root: &XmlNode, sel: &str) -> Result<Vec<TreeOp>, CoreError> {
    let target = unique_match(root, sel)?;
    if let Some(attr) = target.attr.as_deref() {
        return Ok(vec![TreeOp::RemoveAttr {
            path: target.node_path()?.to_op_path(),
            name: attr.to_string(),
        }]);
    }
    let path = target.node_path()?;
    if path.is_root() {
        return Err(CoreError::patch("cannot remove the document element"));
    }
    let index = tree::child_index(root, &path)?;
    let parent = path.parent().expect("not root");
    Ok(vec![TreeOp::RemoveChild {
        path: parent.to_op_path(),
        index,
    }])
}

fn payload_xml(xml: Option<&str>, text: Option<&str>) -> Result<String, CoreError> {
    if let Some(x) = xml {
        return Ok(x.to_string());
    }
    if let Some(t) = text {
        return Ok(t.to_string());
    }
    Err(CoreError::patch("add/replace needs xml or text"))
}

fn insert_ops(
    root: &XmlNode,
    target: &Match,
    pos: PatchPos,
    payload: &str,
) -> Result<Vec<TreeOp>, CoreError> {
    let fragments = tree::parse_fragments(payload)?;
    if fragments.is_empty() {
        return Err(CoreError::patch("add payload is empty"));
    }
    let path = target.node_path()?;
    let mut ops = Vec::new();
    match pos {
        PatchPos::Append => {
            for frag in &fragments {
                ops.push(TreeOp::InsertChild {
                    path: path.to_op_path(),
                    index: None,
                    xml: frag.to_xml_string(),
                });
            }
        }
        PatchPos::Prepend => {
            for (i, frag) in fragments.iter().enumerate() {
                ops.push(TreeOp::InsertChild {
                    path: path.to_op_path(),
                    index: Some(i),
                    xml: frag.to_xml_string(),
                });
            }
        }
        PatchPos::Before | PatchPos::After => {
            if path.is_root() {
                return Err(CoreError::patch(
                    "cannot insert before/after the document element",
                ));
            }
            let mut index = tree::child_index(root, &path)?;
            if pos == PatchPos::After {
                index += 1;
            }
            let parent = path.parent().expect("not root").to_op_path();
            for (i, frag) in fragments.iter().enumerate() {
                ops.push(TreeOp::InsertChild {
                    path: parent.clone(),
                    index: Some(index + i),
                    xml: frag.to_xml_string(),
                });
            }
        }
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::eval_str;
    use crate::xml;

    fn sample() -> XmlNode {
        xml::parse(b"<root><p id=\"a\">one</p><p id=\"b\">two</p></root>")
            .unwrap()
            .root
    }

    #[test]
    fn remove_unique_then_add() {
        let mut root = sample();
        apply(
            &mut root,
            &parse_ops(
                r#"[{"op":"remove","sel":"//p[@id=\"a\"]"},{"op":"add","sel":"/","xml":"<p id=\"c\">three</p>"}]"#,
            )
            .unwrap(),
            true,
        )
        .unwrap();
        assert_eq!(eval_str(&root, "count(//p)").unwrap().as_number(), Some(2));
        assert_eq!(
            eval_str(&root, r#"text(//p[@id="c"])"#).unwrap().to_text(),
            "three"
        );
    }

    #[test]
    fn rfc_xml_add_attr_and_replace() {
        let mut root = sample();
        let xml = r#"<diff>
            <add sel="/p[0]" type="@n">9</add>
            <replace sel="/p[1]"><p id="b">TWO</p></replace>
        </diff>"#;
        apply(&mut root, &parse_rfc_xml(xml).unwrap(), true).unwrap();
        assert_eq!(root.child_named("p", 0).unwrap().get_attr("n"), Some("9"));
        assert_eq!(root.child_named("p", 1).unwrap().text_content(), "TWO");
    }

    #[test]
    fn unique_sel_is_required() {
        let root = sample();
        let err = plan(
            &root,
            &parse_ops(r#"[{"op":"remove","sel":"//p"}]"#).unwrap(),
        );
        assert!(err.unwrap_err().to_string().contains("matched 2"));
    }

    #[test]
    fn insert_before_sibling() {
        let mut root = sample();
        apply(
            &mut root,
            &[PatchOp::Add {
                sel: "/p[1]".into(),
                pos: PatchPos::Before,
                type_attr: None,
                xml: Some("<p id=\"m\">mid</p>".into()),
                text: None,
            }],
            true,
        )
        .unwrap();
        assert_eq!(root.child_named("p", 1).unwrap().text_content(), "mid");
    }

    #[test]
    fn remove_attribute_via_sel() {
        let mut root = sample();
        apply(
            &mut root,
            &[PatchOp::Remove {
                sel: "/p[0]/@id".into(),
            }],
            true,
        )
        .unwrap();
        assert!(root.child_named("p", 0).unwrap().get_attr("id").is_none());
    }
}
