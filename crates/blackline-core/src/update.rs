//! XQuery Update Facility verbs, compiled to [`TreeOp`].
//!
//! Sibling of [`crate::patch`]: same destination (`TreeOp`), different
//! surface. [XQuery Update 1.0](https://www.w3.org/TR/xqupdate/) is
//! `insert` / `delete` / `replace` / `replace value of` / `rename`.
//! SuperDoc's query-then-mutate is the same split — a selector plus a
//! verb.
//!
//! Semantics (documented, not hidden):
//!
//! - Statements run **in order**. Each statement sees the tree left by
//!   the previous one.
//! - Inside one `delete nodes` (many matches), targets are removed last
//!   to first so sibling indexes stay valid.
//! - `delete` of zero nodes is a no-op. `insert` / `replace` / `rename`
//!   require at least one target.
//!
//! JSON (`action`) and the XQuery-like text form are both accepted.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::formula::{self, Match};
use crate::tree::{self, TreeOp, TreeOpReport};
use crate::xml::XmlNode;

/// Where an insert places the source relative to each target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InsertPos {
    /// Last child (`insert node … into`).
    #[default]
    Into,
    /// First child (`as first into`).
    AsFirst,
    /// Last child, explicit (`as last into`).
    AsLast,
    /// Immediate preceding sibling.
    Before,
    /// Immediate following sibling.
    After,
}

/// One update statement, JSON form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum UpdateOp {
    /// `insert node Source (as first|as last)? (into|before|after) Target`
    Insert {
        /// Target selector.
        sel: String,
        /// Position. Default [`InsertPos::Into`].
        #[serde(default)]
        pos: InsertPos,
        /// Element fragment(s).
        #[serde(default)]
        xml: Option<String>,
        /// Text node.
        #[serde(default)]
        text: Option<String>,
    },
    /// `delete node(s) Target`
    Delete {
        /// Target selector. Zero matches is a no-op.
        sel: String,
    },
    /// `replace node Target with Source`
    Replace {
        /// Target selector.
        sel: String,
        /// Replacement fragment.
        #[serde(default)]
        xml: Option<String>,
        /// Replacement text node.
        #[serde(default)]
        text: Option<String>,
    },
    /// `replace value of node Target with Expr`
    ReplaceValue {
        /// Target selector.
        sel: String,
        /// New string value.
        text: String,
    },
    /// `rename node Target as Name`
    Rename {
        /// Target selector.
        sel: String,
        /// New qname.
        name: String,
    },
}

/// Parse `--ops` as JSON or as XQuery Update text (`delete node //ins; …`).
pub fn parse_ops(raw: &str) -> Result<Vec<UpdateOp>, CoreError> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') {
        serde_json::from_str(trimmed)
            .map_err(|e| CoreError::update(format!("invalid update JSON: {e}")))
    } else {
        parse_exprs(trimmed)
    }
}

/// Compile and apply statements in order.
pub fn apply(
    root: &mut XmlNode,
    ops: &[UpdateOp],
    strict: bool,
) -> Result<Vec<TreeOpReport>, CoreError> {
    let planned = plan(root, ops)?;
    tree::apply_ops(root, &planned, strict)
}

/// Compile statements against a clone so later selectors see earlier edits.
pub fn plan(root: &XmlNode, ops: &[UpdateOp]) -> Result<Vec<TreeOp>, CoreError> {
    let mut clone = root.clone();
    let mut all = Vec::new();
    for op in ops {
        let compiled = compile_one(&clone, op)?;
        tree::apply_ops(&mut clone, &compiled, true)?;
        all.extend(compiled);
    }
    Ok(all)
}

fn compile_one(root: &XmlNode, op: &UpdateOp) -> Result<Vec<TreeOp>, CoreError> {
    match op {
        UpdateOp::Insert {
            sel,
            pos,
            xml,
            text,
        } => compile_insert(root, sel, *pos, xml.as_deref(), text.as_deref()),
        UpdateOp::Delete { sel } => compile_delete(root, sel),
        UpdateOp::Replace { sel, xml, text } => {
            compile_replace(root, sel, xml.as_deref(), text.as_deref())
        }
        UpdateOp::ReplaceValue { sel, text } => compile_replace_value(root, sel, text),
        UpdateOp::Rename { sel, name } => compile_rename(root, sel, name),
    }
}

fn matches_or_none(root: &XmlNode, sel: &str) -> Result<Vec<Match>, CoreError> {
    formula::select_str(root, sel)
}

fn matches_required(root: &XmlNode, sel: &str, verb: &str) -> Result<Vec<Match>, CoreError> {
    let ms = matches_or_none(root, sel)?;
    if ms.is_empty() {
        return Err(CoreError::update(format!(
            "{verb}: selector '{sel}' matched nothing"
        )));
    }
    Ok(ms)
}

fn payload(xml: Option<&str>, text: Option<&str>) -> Result<String, CoreError> {
    if let Some(x) = xml {
        return Ok(x.to_string());
    }
    if let Some(t) = text {
        return Ok(t.to_string());
    }
    Err(CoreError::update("insert/replace needs xml or text"))
}

fn compile_insert(
    root: &XmlNode,
    sel: &str,
    pos: InsertPos,
    xml: Option<&str>,
    text: Option<&str>,
) -> Result<Vec<TreeOp>, CoreError> {
    let targets = matches_required(root, sel, "insert")?;
    let body = payload(xml, text)?;
    let fragments = tree::parse_fragments(&body)?;
    if fragments.is_empty() {
        return Err(CoreError::update("insert payload is empty"));
    }
    let mut ops = Vec::new();
    for target in &targets {
        if target.attr.is_some() {
            return Err(CoreError::update(
                "insert targets an element, not an attribute",
            ));
        }
        let path = target.node_path()?;
        match pos {
            InsertPos::Into | InsertPos::AsLast => {
                for frag in &fragments {
                    ops.push(TreeOp::InsertChild {
                        path: path.to_op_path(),
                        index: None,
                        xml: frag.to_xml_string(),
                    });
                }
            }
            InsertPos::AsFirst => {
                for (i, frag) in fragments.iter().enumerate() {
                    ops.push(TreeOp::InsertChild {
                        path: path.to_op_path(),
                        index: Some(i),
                        xml: frag.to_xml_string(),
                    });
                }
            }
            InsertPos::Before | InsertPos::After => {
                if path.is_root() {
                    return Err(CoreError::update(
                        "cannot insert before/after the document element",
                    ));
                }
                let mut index = tree::child_index(root, &path)?;
                if pos == InsertPos::After {
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
    }
    Ok(ops)
}

fn compile_delete(root: &XmlNode, sel: &str) -> Result<Vec<TreeOp>, CoreError> {
    let targets = matches_or_none(root, sel)?;
    let mut scored = Vec::new();
    for target in targets {
        if let Some(attr) = target.attr.as_deref() {
            scored.push((
                target.node_path()?.to_op_path(),
                usize::MAX,
                TreeOp::RemoveAttr {
                    path: target.node_path()?.to_op_path(),
                    name: attr.to_string(),
                },
            ));
            continue;
        }
        let path = target.node_path()?;
        if path.is_root() {
            return Err(CoreError::update("cannot delete the document element"));
        }
        let index = tree::child_index(root, &path)?;
        let parent = path.parent().expect("not root").to_op_path();
        scored.push((
            parent.clone(),
            index,
            TreeOp::RemoveChild {
                path: parent,
                index,
            },
        ));
    }
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).reverse());
    Ok(scored.into_iter().map(|(_, _, op)| op).collect())
}

fn compile_replace(
    root: &XmlNode,
    sel: &str,
    xml: Option<&str>,
    text: Option<&str>,
) -> Result<Vec<TreeOp>, CoreError> {
    let targets = matches_required(root, sel, "replace")?;
    let body = payload(xml, text)?;
    let mut ops = Vec::new();
    for target in targets {
        if target.attr.is_some() {
            return Err(CoreError::update(
                "replace node cannot target an attribute; use replace value of",
            ));
        }
        ops.push(TreeOp::Replace {
            path: target.node_path()?.to_op_path(),
            xml: body.clone(),
        });
    }
    Ok(ops)
}

fn compile_replace_value(root: &XmlNode, sel: &str, text: &str) -> Result<Vec<TreeOp>, CoreError> {
    let targets = matches_required(root, sel, "replace value")?;
    let mut ops = Vec::new();
    for target in targets {
        if let Some(attr) = target.attr.as_deref() {
            ops.push(TreeOp::SetAttr {
                path: target.node_path()?.to_op_path(),
                name: attr.to_string(),
                value: text.to_string(),
            });
        } else {
            ops.push(TreeOp::SetText {
                path: target.node_path()?.to_op_path(),
                text: text.to_string(),
            });
        }
    }
    Ok(ops)
}

fn compile_rename(root: &XmlNode, sel: &str, name: &str) -> Result<Vec<TreeOp>, CoreError> {
    let targets = matches_required(root, sel, "rename")?;
    let mut ops = Vec::new();
    for target in targets {
        if target.attr.is_some() {
            return Err(CoreError::update(
                "rename targets an element, not an attribute",
            ));
        }
        ops.push(TreeOp::Rename {
            path: target.node_path()?.to_op_path(),
            name: name.to_string(),
        });
    }
    Ok(ops)
}

// -----------------------------------------------------------------------------
// Text parser
// -----------------------------------------------------------------------------

/// Parse `delete node //ins; insert node <x/> into /body`.
pub fn parse_exprs(src: &str) -> Result<Vec<UpdateOp>, CoreError> {
    let mut p = Parser::new(src);
    let mut ops = Vec::new();
    loop {
        p.skip_ws();
        if p.eof() {
            break;
        }
        ops.push(p.parse_stmt()?);
        p.skip_ws();
        let _ = p.eat(";");
    }
    if ops.is_empty() {
        return Err(CoreError::update("empty update expression"));
    }
    Ok(ops)
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    fn eof(&self) -> bool {
        self.rest().trim_start().is_empty()
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.bump();
        }
    }

    fn eat(&mut self, s: &str) -> bool {
        self.skip_ws();
        if self.rest().starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn error(&self, msg: impl Into<String>) -> CoreError {
        let rest = self.rest().chars().take(24).collect::<String>();
        CoreError::update(format!("{} near '{rest}'", msg.into()))
    }

    fn expect_kw(&mut self, kw: &str) -> Result<(), CoreError> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            Err(self.error(format!("expected '{kw}'")))
        }
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        self.skip_ws();
        let ident = peek_ident(self.rest());
        if ident == kw {
            self.pos += kw.len();
            true
        } else {
            false
        }
    }

    fn parse_stmt(&mut self) -> Result<UpdateOp, CoreError> {
        self.skip_ws();
        if self.eat_kw("insert") {
            return self.parse_insert();
        }
        if self.eat_kw("delete") {
            return self.parse_delete();
        }
        if self.eat_kw("replace") {
            return self.parse_replace();
        }
        if self.eat_kw("rename") {
            return self.parse_rename();
        }
        Err(self.error("expected insert, delete, replace, or rename"))
    }

    fn parse_node_kw(&mut self) -> Result<(), CoreError> {
        if self.eat_kw("node") || self.eat_kw("nodes") {
            Ok(())
        } else {
            Err(self.error("expected 'node' or 'nodes'"))
        }
    }

    fn parse_insert(&mut self) -> Result<UpdateOp, CoreError> {
        self.parse_node_kw()?;
        let (xml, text) = self.parse_source()?;
        let pos = if self.eat_kw("as") {
            let pos = if self.eat_kw("first") {
                InsertPos::AsFirst
            } else if self.eat_kw("last") {
                InsertPos::AsLast
            } else {
                return Err(self.error("expected 'first' or 'last'"));
            };
            self.expect_kw("into")?;
            pos
        } else if self.eat_kw("into") {
            InsertPos::Into
        } else if self.eat_kw("before") {
            InsertPos::Before
        } else if self.eat_kw("after") {
            InsertPos::After
        } else {
            return Err(self.error("expected into, before, after, or as first/last into"));
        };
        let sel = self.parse_selector_text()?;
        Ok(UpdateOp::Insert {
            sel,
            pos,
            xml,
            text,
        })
    }

    fn parse_delete(&mut self) -> Result<UpdateOp, CoreError> {
        self.parse_node_kw()?;
        let sel = self.parse_selector_text()?;
        Ok(UpdateOp::Delete { sel })
    }

    fn parse_replace(&mut self) -> Result<UpdateOp, CoreError> {
        if self.eat_kw("value") {
            self.expect_kw("of")?;
            self.parse_node_kw()?;
            let sel = self.parse_selector_text()?;
            self.expect_kw("with")?;
            let (xml, text) = self.parse_source()?;
            let value = text.or(xml).unwrap_or_default();
            return Ok(UpdateOp::ReplaceValue { sel, text: value });
        }
        self.parse_node_kw()?;
        let sel = self.parse_selector_text()?;
        self.expect_kw("with")?;
        let (xml, text) = self.parse_source()?;
        Ok(UpdateOp::Replace { sel, xml, text })
    }

    fn parse_rename(&mut self) -> Result<UpdateOp, CoreError> {
        self.parse_node_kw()?;
        let sel = self.parse_selector_text()?;
        self.expect_kw("as")?;
        self.skip_ws();
        let name = peek_ident(self.rest()).to_string();
        if name.is_empty() {
            return Err(self.error("expected a name after 'as'"));
        }
        self.pos += name.len();
        let mut qname = name;
        if self.eat(":") {
            let local = peek_ident(self.rest()).to_string();
            if local.is_empty() {
                return Err(self.error("expected local name after ':'"));
            }
            self.pos += local.len();
            qname = format!("{qname}:{local}");
        }
        Ok(UpdateOp::Rename { sel, name: qname })
    }

    fn parse_source(&mut self) -> Result<(Option<String>, Option<String>), CoreError> {
        self.skip_ws();
        match self.peek() {
            Some('"') | Some('\'') => Ok((None, Some(self.parse_string()?))),
            Some('<') => Ok((Some(self.parse_xml_fragment()?), None)),
            _ => Err(self.error("expected a quoted string or an XML fragment")),
        }
    }

    fn parse_string(&mut self) -> Result<String, CoreError> {
        self.skip_ws();
        let quote = self
            .bump()
            .filter(|c| *c == '"' || *c == '\'')
            .ok_or_else(|| self.error("expected string"))?;
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == quote {
                let s = self.src[start..self.pos].to_string();
                self.bump();
                return Ok(s);
            }
            self.bump();
        }
        Err(self.error("unterminated string"))
    }

    fn parse_xml_fragment(&mut self) -> Result<String, CoreError> {
        self.skip_ws();
        if !self.rest().starts_with('<') {
            return Err(self.error("expected '<'"));
        }
        let start = self.pos;
        let mut depth = 0i32;
        let bytes = self.rest().as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'<' {
                if bytes[i..].starts_with(b"</") {
                    depth -= 1;
                    i += 2;
                    while i < bytes.len() && bytes[i] != b'>' {
                        i += 1;
                    }
                    i += 1;
                    if depth == 0 {
                        self.pos = start + i;
                        return Ok(self.src[start..self.pos].to_string());
                    }
                    continue;
                }
                if bytes[i..].starts_with(b"<!--") {
                    if let Some(end) = self.rest()[i..].find("-->") {
                        i += end + 3;
                        continue;
                    }
                    return Err(self.error("unterminated comment"));
                }
                depth += 1;
                let mut j = i + 1;
                let mut self_close = false;
                while j < bytes.len() {
                    if bytes[j] == b'"' || bytes[j] == b'\'' {
                        let q = bytes[j];
                        j += 1;
                        while j < bytes.len() && bytes[j] != q {
                            j += 1;
                        }
                        j += 1;
                        continue;
                    }
                    if bytes[j] == b'/' && j + 1 < bytes.len() && bytes[j + 1] == b'>' {
                        self_close = true;
                        j += 2;
                        break;
                    }
                    if bytes[j] == b'>' {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                i = j;
                if self_close {
                    depth -= 1;
                    if depth == 0 {
                        self.pos = start + i;
                        return Ok(self.src[start..self.pos].to_string());
                    }
                }
            } else {
                i += 1;
            }
        }
        Err(self.error("unterminated XML fragment"))
    }

    fn parse_selector_text(&mut self) -> Result<String, CoreError> {
        self.skip_ws();
        let start = self.pos;
        let rest = self.rest().to_string();
        let mut end = 0usize;
        let mut quote: Option<char> = None;
        let mut brackets = 0i32;
        for (idx, c) in rest.char_indices() {
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
                end = idx + c.len_utf8();
                continue;
            }
            if c == '"' || c == '\'' {
                quote = Some(c);
                end = idx + c.len_utf8();
                continue;
            }
            if c == '[' {
                brackets += 1;
                end = idx + 1;
                continue;
            }
            if c == ']' {
                brackets -= 1;
                end = idx + 1;
                continue;
            }
            if brackets == 0
                && (c == ';' || keyword_at(&rest, idx, "with") || keyword_at(&rest, idx, "as"))
            {
                break;
            }
            end = idx + c.len_utf8();
        }
        let raw = rest[..end].trim_end();
        let sel = raw.trim();
        if sel.is_empty() {
            return Err(self.error("expected a selector"));
        }
        formula::parse_selector(sel).map_err(|e| CoreError::update(e.to_string()))?;
        self.pos = start + raw.len();
        Ok(sel.to_string())
    }
}

fn keyword_at(src: &str, idx: usize, kw: &str) -> bool {
    let rest = &src[idx..];
    if !rest.starts_with(kw) {
        return false;
    }
    let before_ok = idx == 0
        || src[..idx]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_whitespace());
    let after = &rest[kw.len()..];
    let after_ok = after.is_empty()
        || after.starts_with(|c: char| c.is_whitespace() || c == ';' || c == '/' || c == '.');
    before_ok && after_ok
}

fn peek_ident(s: &str) -> &str {
    let mut chars = s.char_indices();
    let Some((_, c)) = chars.next() else {
        return "";
    };
    if !c.is_ascii_alphabetic() && c != '_' {
        return "";
    }
    let end = chars
        .find(|(_, c)| !c.is_ascii_alphanumeric() && *c != '_' && *c != '-')
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    &s[..end]
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
    fn delete_nodes_text() {
        let mut root = sample();
        apply(&mut root, &parse_exprs("delete nodes //p").unwrap(), true).unwrap();
        assert_eq!(eval_str(&root, "count(//p)").unwrap().as_number(), Some(0));
    }

    #[test]
    fn insert_into_and_replace_value() {
        let mut root = sample();
        apply(
            &mut root,
            &parse_exprs(
                r#"insert node <q>x</q> as first into /; replace value of node //p[1] with "TWO""#,
            )
            .unwrap(),
            true,
        )
        .unwrap();
        assert_eq!(root.children()[0].local_name(), "q");
        assert_eq!(root.child_named("p", 1).unwrap().text_content(), "TWO");
    }

    #[test]
    fn rename_and_json() {
        let mut root = sample();
        apply(
            &mut root,
            &parse_ops(r#"[{"action":"rename","sel":"//p[0]","name":"q"}]"#).unwrap(),
            true,
        )
        .unwrap();
        assert_eq!(root.child_named("q", 0).unwrap().text_content(), "one");
    }

    #[test]
    fn replace_node_and_insert_before() {
        let mut root = sample();
        apply(
            &mut root,
            &parse_exprs(r#"replace node //p[0] with <p id="a">ONE</p>; insert node <p id="m">mid</p> before //p[1]"#)
                .unwrap(),
            true,
        )
        .unwrap();
        assert_eq!(root.child_named("p", 0).unwrap().text_content(), "ONE");
        assert_eq!(root.child_named("p", 1).unwrap().text_content(), "mid");
    }

    #[test]
    fn replace_value_of_attr() {
        let mut root = sample();
        apply(
            &mut root,
            &parse_exprs(r#"replace value of node //p[0]/@id with "z""#).unwrap(),
            true,
        )
        .unwrap();
        assert_eq!(root.child_named("p", 0).unwrap().get_attr("id"), Some("z"));
    }
}
