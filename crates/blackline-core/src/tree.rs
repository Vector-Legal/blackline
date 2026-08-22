//! Path-addressed XML tree mutations.
//!
//! This is the unopinionated edit layer. Format-specific operations
//! (replace a phrase, wrap a tracked insertion, set a cell) compile down
//! to [`TreeOp`]s against a part's [`crate::XmlDocument`].

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::xml::XmlNode;

/// One step in a node path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathStep {
    /// The `n`th child node (any kind), 0-based.
    Index(usize),
    /// The `n`th child *element* with this local name, 0-based.
    Name { local: String, index: usize },
}

/// A path from a document root to a descendant.
///
/// String form accepts either style, mixed:
///
/// ```text
/// document/body/p[2]/r[0]/t
/// 0/1/3
/// document/0/t
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePath {
    /// Path steps from the root (root itself is implied; the first step
    /// selects a child of the root, or the root when the path is empty).
    pub steps: Vec<PathStep>,
}

impl NodePath {
    /// Empty path — the root itself.
    pub fn root() -> Self {
        Self { steps: Vec::new() }
    }

    /// Parse a `/`-separated path string.
    pub fn parse(s: &str) -> Result<Self, CoreError> {
        let s = s.trim().trim_start_matches('/');
        if s.is_empty() {
            return Ok(Self::root());
        }
        let mut steps = Vec::new();
        for raw in s.split('/') {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            steps.push(parse_step(raw)?);
        }
        Ok(Self { steps })
    }

    /// Resolve this path against `root`. The empty path returns `root`.
    pub fn resolve<'a>(&self, root: &'a XmlNode) -> Result<&'a XmlNode, CoreError> {
        let mut cur = root;
        for (i, step) in self.steps.iter().enumerate() {
            cur = step_get(cur, step).ok_or_else(|| {
                CoreError::Path(format!(
                    "failed at step {i} ({step:?}) in {}",
                    self.display()
                ))
            })?;
        }
        Ok(cur)
    }

    /// Mutable resolve.
    pub fn resolve_mut<'a>(&self, root: &'a mut XmlNode) -> Result<&'a mut XmlNode, CoreError> {
        let mut cur = root;
        for (i, step) in self.steps.iter().enumerate() {
            let next_exists = step_get(cur, step).is_some();
            if !next_exists {
                return Err(CoreError::Path(format!(
                    "failed at step {i} ({step:?}) in {}",
                    self.display()
                )));
            }
            cur = step_get_mut(cur, step).expect("checked");
        }
        Ok(cur)
    }

    /// Human-readable form.
    pub fn display(&self) -> String {
        if self.steps.is_empty() {
            return "/".into();
        }
        let mut out = String::new();
        for step in &self.steps {
            out.push('/');
            match step {
                PathStep::Index(i) => out.push_str(&i.to_string()),
                PathStep::Name { local, index } => {
                    out.push_str(local);
                    out.push('[');
                    out.push_str(&index.to_string());
                    out.push(']');
                }
            }
        }
        out
    }

    /// True when this path is the document element.
    pub fn is_root(&self) -> bool {
        self.steps.is_empty()
    }

    /// Parent path, or `None` at the root.
    pub fn parent(&self) -> Option<Self> {
        if self.steps.is_empty() {
            None
        } else {
            Some(Self {
                steps: self.steps[..self.steps.len() - 1].to_vec(),
            })
        }
    }

    /// Path string for a [`TreeOp`] (`""` at the root, no leading slash).
    pub fn to_op_path(&self) -> String {
        if self.steps.is_empty() {
            String::new()
        } else {
            self.display().trim_start_matches('/').to_string()
        }
    }
}

/// Raw child index of `path` among its parent's children (all kinds).
///
/// [`TreeOp::RemoveChild`] and [`TreeOp::InsertChild`] use this index space,
/// not the named `p[0]` occurrence.
pub fn child_index(root: &XmlNode, path: &NodePath) -> Result<usize, CoreError> {
    let parent_path = path
        .parent()
        .ok_or_else(|| CoreError::Path("root has no parent index".into()))?;
    let parent = parent_path.resolve(root)?;
    let last = path
        .steps
        .last()
        .ok_or_else(|| CoreError::Path("root has no parent index".into()))?;
    match last {
        PathStep::Index(i) => {
            if *i < parent.children().len() {
                Ok(*i)
            } else {
                Err(CoreError::Path(format!(
                    "index {i} out of range on {}",
                    parent_path.display()
                )))
            }
        }
        PathStep::Name { local, index } => {
            let mut seen = 0usize;
            for (i, child) in parent.children().iter().enumerate() {
                if child.is_element_with_local_name(local) {
                    if seen == *index {
                        return Ok(i);
                    }
                    seen += 1;
                }
            }
            Err(CoreError::Path(format!(
                "{local}[{index}] missing on {}",
                parent_path.display()
            )))
        }
    }
}

fn parse_step(raw: &str) -> Result<PathStep, CoreError> {
    if let Some((name, rest)) = raw.split_once('[') {
        let idx = rest
            .trim_end_matches(']')
            .parse::<usize>()
            .map_err(|_| CoreError::Path(format!("invalid index in path step '{raw}'")))?;
        if name.chars().all(|c| c.is_ascii_digit()) {
            return Err(CoreError::Path(format!(
                "numeric name with bracket is ambiguous: '{raw}'"
            )));
        }
        let local = name.split(':').next_back().unwrap_or(name).to_string();
        return Ok(PathStep::Name { local, index: idx });
    }
    if raw.chars().all(|c| c.is_ascii_digit()) {
        let i = raw
            .parse::<usize>()
            .map_err(|_| CoreError::Path(format!("invalid index '{raw}'")))?;
        return Ok(PathStep::Index(i));
    }
    let local = raw.split(':').next_back().unwrap_or(raw).to_string();
    Ok(PathStep::Name { local, index: 0 })
}

fn step_get<'a>(node: &'a XmlNode, step: &PathStep) -> Option<&'a XmlNode> {
    match step {
        PathStep::Index(i) => node.children().get(*i),
        PathStep::Name { local, index } => node.child_named(local, *index),
    }
}

fn step_get_mut<'a>(node: &'a mut XmlNode, step: &PathStep) -> Option<&'a mut XmlNode> {
    match step {
        PathStep::Index(i) => node.try_children_mut()?.get_mut(*i),
        PathStep::Name { local, index } => node.child_named_mut(local, *index),
    }
}

/// A single tree mutation. Deserializable from JSON so the CLI and agents
/// can drive the XML layer directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum TreeOp {
    /// Set or insert an attribute on the node at `path`.
    #[serde(rename = "set_attr")]
    SetAttr {
        /// Node path.
        path: String,
        /// Attribute name (`w:val`, `xml:space`, …).
        name: String,
        /// Attribute value.
        value: String,
    },
    /// Remove an attribute.
    #[serde(rename = "remove_attr")]
    RemoveAttr {
        /// Node path.
        path: String,
        /// Attribute name.
        name: String,
    },
    /// Replace the node's children with a text node.
    #[serde(rename = "set_text")]
    SetText {
        /// Node path.
        path: String,
        /// New text.
        text: String,
    },
    /// Insert a child at `index` (0-based). The child is parsed as an XML
    /// fragment (a single element) or used as a text node when it does not
    /// start with `<`.
    #[serde(rename = "insert_child")]
    InsertChild {
        /// Parent path.
        path: String,
        /// Insertion index. Omit to append.
        #[serde(default)]
        index: Option<usize>,
        /// XML fragment or plain text.
        xml: String,
    },
    /// Remove the child at `index`.
    #[serde(rename = "remove_child")]
    RemoveChild {
        /// Parent path.
        path: String,
        /// Child index.
        index: usize,
    },
    /// Replace the node at `path` with a parsed fragment.
    #[serde(rename = "replace")]
    Replace {
        /// Node path.
        path: String,
        /// XML fragment or plain text.
        xml: String,
    },
    /// Change the element's qualified name. Children and attributes stay.
    #[serde(rename = "rename")]
    Rename {
        /// Node path.
        path: String,
        /// New qname (`w:p`, `p`, …).
        name: String,
    },
}

/// Result of applying one [`TreeOp`].
#[derive(Debug, Clone, Serialize)]
pub struct TreeOpReport {
    /// Zero-based position in the submitted batch.
    pub index: usize,
    /// Action name.
    pub action: String,
    /// `applied` or `failed`.
    pub status: &'static str,
    /// Detail or failure reason.
    pub detail: String,
}

/// Apply `ops` to `root`. Stops on the first failure when `strict` is true.
pub fn apply_ops(
    root: &mut XmlNode,
    ops: &[TreeOp],
    strict: bool,
) -> Result<Vec<TreeOpReport>, CoreError> {
    let mut reports = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        match apply_one(root, op) {
            Ok(detail) => reports.push(TreeOpReport {
                index: i,
                action: action_name(op).into(),
                status: "applied",
                detail,
            }),
            Err(e) => {
                reports.push(TreeOpReport {
                    index: i,
                    action: action_name(op).into(),
                    status: "failed",
                    detail: e.to_string(),
                });
                if strict {
                    return Err(e);
                }
            }
        }
    }
    Ok(reports)
}

fn action_name(op: &TreeOp) -> &'static str {
    match op {
        TreeOp::SetAttr { .. } => "set_attr",
        TreeOp::RemoveAttr { .. } => "remove_attr",
        TreeOp::SetText { .. } => "set_text",
        TreeOp::InsertChild { .. } => "insert_child",
        TreeOp::RemoveChild { .. } => "remove_child",
        TreeOp::Replace { .. } => "replace",
        TreeOp::Rename { .. } => "rename",
    }
}

fn apply_one(root: &mut XmlNode, op: &TreeOp) -> Result<String, CoreError> {
    match op {
        TreeOp::SetAttr { path, name, value } => {
            let node = NodePath::parse(path)?.resolve_mut(root)?;
            node.set_attr(name, value);
            Ok(format!("set {name} on {path}"))
        }
        TreeOp::RemoveAttr { path, name } => {
            let node = NodePath::parse(path)?.resolve_mut(root)?;
            node.remove_attr(name);
            Ok(format!("removed {name} from {path}"))
        }
        TreeOp::SetText { path, text } => {
            let node = NodePath::parse(path)?.resolve_mut(root)?;
            node.set_text(text);
            Ok(format!("set text on {path}"))
        }
        TreeOp::InsertChild { path, index, xml } => {
            let child = parse_fragment(xml)?;
            let node = NodePath::parse(path)?.resolve_mut(root)?;
            let kids = node
                .try_children_mut()
                .ok_or_else(|| CoreError::Path(format!("{path} is not an element")))?;
            let i = index.unwrap_or(kids.len());
            if i > kids.len() {
                return Err(CoreError::Path(format!(
                    "insert index {i} out of range (len {})",
                    kids.len()
                )));
            }
            kids.insert(i, child);
            Ok(format!("inserted child at {path}[{i}]"))
        }
        TreeOp::RemoveChild { path, index } => {
            let node = NodePath::parse(path)?.resolve_mut(root)?;
            let kids = node
                .try_children_mut()
                .ok_or_else(|| CoreError::Path(format!("{path} is not an element")))?;
            if *index >= kids.len() {
                return Err(CoreError::Path(format!(
                    "remove index {index} out of range (len {})",
                    kids.len()
                )));
            }
            kids.remove(*index);
            Ok(format!("removed child {index} of {path}"))
        }
        TreeOp::Replace { path, xml } => {
            let parsed = NodePath::parse(path)?;
            if parsed.steps.is_empty() {
                *root = parse_fragment(xml)?;
                return Ok("replaced root".into());
            }
            let replacement = parse_fragment(xml)?;
            let parent_path = NodePath {
                steps: parsed.steps[..parsed.steps.len() - 1].to_vec(),
            };
            let last = parsed.steps.last().cloned().unwrap();
            let parent = parent_path.resolve_mut(root)?;
            match last {
                PathStep::Index(i) => {
                    let kids = parent
                        .try_children_mut()
                        .ok_or_else(|| CoreError::Path("parent is not an element".into()))?;
                    let slot = kids
                        .get_mut(i)
                        .ok_or_else(|| CoreError::Path(format!("index {i} missing")))?;
                    *slot = replacement;
                }
                PathStep::Name { local, index } => {
                    let slot = parent.child_named_mut(&local, index).ok_or_else(|| {
                        CoreError::Path(format!("{local}[{index}] missing on parent"))
                    })?;
                    *slot = replacement;
                }
            }
            Ok(format!("replaced {path}"))
        }
        TreeOp::Rename { path, name } => {
            let node = NodePath::parse(path)?.resolve_mut(root)?;
            if !node.is_element() {
                return Err(CoreError::Path(format!("{path} is not an element")));
            }
            node.set_qname(name);
            Ok(format!("renamed {path} to {name}"))
        }
    }
}

/// Parse a single-element XML fragment, or treat the string as text.
pub fn parse_fragment(xml: &str) -> Result<XmlNode, CoreError> {
    let trimmed = xml.trim();
    if trimmed.starts_with('<') {
        let wrapped = format!("<_frag>{trimmed}</_frag>");
        let doc = crate::xml::parse(wrapped.as_bytes())?;
        let kids = doc.root.children();
        if kids.len() == 1 {
            return Ok(kids[0].clone());
        }
        return Ok(doc.root);
    }
    Ok(XmlNode::Text(xml.to_string()))
}

/// Parse zero or more sibling fragments. Unlike [`parse_fragment`], a
/// multi-element string does not wrap them in a synthetic `_frag` parent.
pub fn parse_fragments(xml: &str) -> Result<Vec<XmlNode>, CoreError> {
    let trimmed = xml.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('<') {
        let wrapped = format!("<_frag>{trimmed}</_frag>");
        let doc = crate::xml::parse(wrapped.as_bytes())?;
        return Ok(doc.root.children().to_vec());
    }
    Ok(vec![XmlNode::Text(xml.to_string())])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml;

    fn sample() -> XmlNode {
        xml::parse(b"<root><p>one</p><p>two</p></root>")
            .unwrap()
            .root
    }

    #[test]
    fn path_by_name() {
        let root = sample();
        let p = NodePath::parse("p[1]").unwrap();
        assert_eq!(p.resolve(&root).unwrap().text_content(), "two");
    }

    #[test]
    fn path_by_index() {
        let root = sample();
        let p = NodePath::parse("0").unwrap();
        assert_eq!(p.resolve(&root).unwrap().local_name(), "p");
    }

    #[test]
    fn set_text_op() {
        let mut root = sample();
        apply_ops(
            &mut root,
            &[TreeOp::SetText {
                path: "p[0]".into(),
                text: "changed".into(),
            }],
            true,
        )
        .unwrap();
        assert_eq!(root.child_named("p", 0).unwrap().text_content(), "changed");
    }

    #[test]
    fn insert_and_remove() {
        let mut root = sample();
        apply_ops(
            &mut root,
            &[TreeOp::InsertChild {
                path: "".into(),
                index: Some(1),
                xml: "<p>mid</p>".into(),
            }],
            true,
        )
        .unwrap();
        assert_eq!(root.find_all("p").len(), 3);
        apply_ops(
            &mut root,
            &[TreeOp::RemoveChild {
                path: "".into(),
                index: 1,
            }],
            true,
        )
        .unwrap();
        assert_eq!(root.find_all("p").len(), 2);
    }

    #[test]
    fn set_attr_and_replace() {
        let mut root = sample();
        apply_ops(
            &mut root,
            &[
                TreeOp::SetAttr {
                    path: "p[0]".into(),
                    name: "w:val".into(),
                    value: "x".into(),
                },
                TreeOp::Replace {
                    path: "p[1]".into(),
                    xml: "<p>replaced</p>".into(),
                },
            ],
            true,
        )
        .unwrap();
        assert_eq!(root.child_named("p", 0).unwrap().get_attr("val"), Some("x"));
        assert_eq!(root.child_named("p", 1).unwrap().text_content(), "replaced");
    }

    #[test]
    fn missing_path_fails_strict() {
        let mut root = sample();
        let err = apply_ops(
            &mut root,
            &[TreeOp::SetText {
                path: "p[9]".into(),
                text: "x".into(),
            }],
            true,
        );
        assert!(err.is_err());
    }

    #[test]
    fn rename_and_child_index() {
        let mut root = sample();
        let p0 = NodePath::parse("p[0]").unwrap();
        assert_eq!(child_index(&root, &p0).unwrap(), 0);
        apply_ops(
            &mut root,
            &[TreeOp::Rename {
                path: "p[1]".into(),
                name: "q".into(),
            }],
            true,
        )
        .unwrap();
        assert_eq!(root.child_named("q", 0).unwrap().text_content(), "two");
        assert_eq!(root.find_all("p").len(), 1);
    }
}
