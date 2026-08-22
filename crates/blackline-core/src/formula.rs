//! XML formulas: restricted XPath selectors plus a small function language.
//!
//! This is the **query** counterpart to [`crate::tree::TreeOp`]. Selectors
//! locate nodes; formulas reduce a selection to a typed value. Neither
//! mutates the tree. [`crate::patch`] and [`crate::update`] compile
//! verbs plus these selectors into `TreeOp`s.
//!
//! # Lineage
//!
//! The language is a deliberate subset of existing work, not a new query
//! dialect:
//!
//! | Source | What we took |
//! | --- | --- |
//! | [RFC 5261](https://www.rfc-editor.org/rfc/rfc5261.html) XML Patch | Location paths as `sel`: child `/`, descendant `//`, `.` / `..` / `*`, `@attr="value"` predicates. Each path step targets elements. |
//! | [XPath 1.0](https://www.w3.org/TR/xpath-10/) | `count`, `contains`, `starts-with`, `not`, `name` / `local-name`, `concat`, `substring`, `substring-before` / `substring-after`, `string-length`, `normalize-space`, `boolean`, `number`. |
//! | [XQuery Update Facility](https://www.w3.org/TR/xqupdate/) | The insert / delete / replace / replace-value / rename verbs — already [`crate::tree::TreeOp`]. Formulas are the *target* expression those verbs will consume. |
//! | [SuperDoc](https://www.superdoc.dev/) | Query (`query.match`) then mutate. Formulas are that query layer. |
//!
//! We did **not** vendor a full XPath 3.1 engine ([xee](https://github.com/Paligo/xee),
//! Saxon, libxml). Those operate on a different DOM and would hide the
//! [`crate::XmlNode`] / [`crate::NodePath`] contract this crate is built on.
//!
//! Tree-edit-distance redliners (X-Diff, XyDiff, Zhang–Shasha) and
//! word-level visual differs ([DaisyDiff](https://daisydiff.github.io/))
//! belong in [`mod@crate::diff`] / format redline code, not here. DaisyDiff's
//! rule — do not mark a whole paragraph changed when one word moved —
//! is already how tracked replace works.
//!
//! # Selector
//!
//! ```text
//! /body/p[0]/r[0]/t     child axis, 0-based name index (same as NodePath)
//! //ins                 every descendant `ins` (and the root, if it matches)
//! //hyperlink[@id="rId4"]
//! //ins[0]/@author      attribute step (RFC 5261 `sel` ending in `/@name`)
//! //p[last()]           last match of that step
//! .                     the context node (document element when evaluating a part)
//! *                     any element
//! ```
//!
//! **Indexing is 0-based**, matching [`crate::NodePath`] / `TreeOp`, *not*
//! XPath's 1-based `position()`. `p[0]` is the first `p` child. On a
//! descendant step, `//p[0]` is the first `p` in document order.
//!
//! Names match **local name** (`ins` = `w:ins`). A prefix in the formula
//! is accepted and ignored, same as `NodePath::parse`.
//!
//! # Formula
//!
//! ```text
//! count(//ins)
//! exists(//del)
//! text(//p[0])
//! attr(//ins[0], "author")
//! name(//p[0])
//! contains(text(//t), "Notice")
//! starts_with(text(//p[0]), "The")
//! ends_with(text(//p[0]), "days.")
//! concat(text(//p[0]), " ", text(//p[1]))
//! substring(text(//p[0]), 0, 3)
//! normalize_space(text(//p[0]))
//! not(exists(//ins))
//! count(//ins) = 2
//! exists(//ins) or exists(//del)
//! //p                     node-set (a bare selector is a formula)
//! ```
//!
//! Hyphenated XPath spellings (`starts-with`, `local-name`, `string-length`,
//! `normalize-space`, `substring-before`, `ends-with`) are accepted.
//! `substring` start is **0-based**, like `NodePath`, not XPath 1.0.
//!
//! # Values
//!
//! `null` · `bool` · `number` (i64) · `string` · `nodes` (paths + text).
//!
//! Coercion used by `=`, `and`, `or`, `contains`:
//!
//! - nodes → string: concatenated `text_content` in document order
//! - nodes → number: match count
//! - nodes → bool: count > 0

use std::collections::HashSet;

use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;

use crate::error::CoreError;
use crate::tree::{NodePath, PathStep};
use crate::xml::XmlNode;

/// A parsed formula. Evaluate with [`eval`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Formula {
    /// Literal.
    Literal(Literal),
    /// Bare selector — result is a node-set.
    Selector(Selector),
    /// `count`, `text`, `contains`, …
    Call {
        /// Canonical function name (`starts_with`, not `starts-with`).
        name: String,
        /// Arguments.
        args: Vec<Formula>,
    },
    /// `=`, `!=`, `and`, `or`.
    Binary {
        /// Operator.
        op: BinOp,
        /// Left operand.
        left: Box<Formula>,
        /// Right operand.
        right: Box<Formula>,
    },
}

/// Literal operand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    /// `null` is not writable; used internally.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// Integer.
    Number(i64),
    /// `'…'` or `"…"`.
    String(String),
}

/// Binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// `=`
    Eq,
    /// `!=`
    Ne,
    /// `and`
    And,
    /// `or`
    Or,
}

/// RFC 5261-style location path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    /// Location steps, applied left to right from the document element.
    pub steps: Vec<Step>,
    /// Optional trailing `/@attr` (RFC 5261 attribute target).
    pub attr: Option<String>,
}

/// One location step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Child `/` or descendant-or-self `//`. `.` / `..` use [`Axis::Child`]
    /// with a [`NodeTest::SelfNode`] / [`NodeTest::Parent`].
    pub axis: Axis,
    /// What to keep along that axis.
    pub test: NodeTest,
    /// Predicates, left to right.
    pub predicates: Vec<Predicate>,
}

/// Location axis. RFC 5261 only needs child and descendant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// `/` — direct element children of each context node.
    Child,
    /// `//` — the context node and every descendant element.
    Descendant,
}

/// Node test on an axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeTest {
    /// `.`
    SelfNode,
    /// `..`
    Parent,
    /// `*`
    Any,
    /// Local name (`ins`, `p`, …).
    Name(String),
}

/// Predicate on a step. RFC 5261 `[@attr="v"]` plus a 0-based index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    /// `[n]` — 0-based. On a child step, among remaining siblings of that
    /// parent; on a descendant step, among the collected document order.
    Index(usize),
    /// `[@id]` — attribute exists (local or qualified name).
    HasAttr(String),
    /// `[@id="rId4"]`
    AttrEq {
        /// Attribute name.
        name: String,
        /// Exact value.
        value: String,
    },
    /// `[last()]` — last remaining node after earlier predicates.
    Last,
}

/// One selected node, ready to hand to `xml get --path` or a `TreeOp`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Match {
    /// [`NodePath`] display form (`/body/p[0]`).
    pub path: String,
    /// Qualified name (`w:p`) or local name.
    pub name: String,
    /// Concatenated descendant text, or the attribute value when [`Match::attr`]
    /// is set.
    pub text: String,
    /// Trailing `/@attr` target, when the selector addressed an attribute.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attr: Option<String>,
}

impl Match {
    /// Parse [`Match::path`] as a [`NodePath`].
    pub fn node_path(&self) -> Result<NodePath, CoreError> {
        NodePath::parse(self.path.trim_start_matches('/'))
    }
}

/// Result of evaluating a formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Absent / empty.
    Null,
    /// Boolean.
    Bool(bool),
    /// Integer (counts, comparisons).
    Number(i64),
    /// String (text, attr, name).
    String(String),
    /// Selected nodes.
    Nodes(Vec<Match>),
}

impl Value {
    /// XPath-ish boolean coercion.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0,
            Value::String(s) => !s.is_empty(),
            Value::Nodes(ms) => !ms.is_empty(),
        }
    }

    /// Integer if this is a number, or the count of a node-set.
    pub fn as_number(&self) -> Option<i64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Nodes(ms) => Some(ms.len() as i64),
            Value::Bool(true) => Some(1),
            Value::Bool(false) => Some(0),
            _ => None,
        }
    }

    /// Borrowed string when the value is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Node-set matches.
    pub fn matches(&self) -> Option<&[Match]> {
        match self {
            Value::Nodes(ms) => Some(ms),
            _ => None,
        }
    }

    /// Coerce to string. Node-sets concatenate text in document order.
    pub fn to_text(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            Value::Nodes(ms) => ms.iter().map(|m| m.text.as_str()).collect(),
        }
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Value::Null => {
                let mut s = serializer.serialize_struct("Value", 1)?;
                s.serialize_field("kind", "null")?;
                s.end()
            }
            Value::Bool(v) => {
                let mut s = serializer.serialize_struct("Value", 2)?;
                s.serialize_field("kind", "bool")?;
                s.serialize_field("value", v)?;
                s.end()
            }
            Value::Number(v) => {
                let mut s = serializer.serialize_struct("Value", 2)?;
                s.serialize_field("kind", "number")?;
                s.serialize_field("value", v)?;
                s.end()
            }
            Value::String(v) => {
                let mut s = serializer.serialize_struct("Value", 2)?;
                s.serialize_field("kind", "string")?;
                s.serialize_field("value", v)?;
                s.end()
            }
            Value::Nodes(ms) => {
                let mut s = serializer.serialize_struct("Value", 3)?;
                s.serialize_field("kind", "nodes")?;
                s.serialize_field("count", &ms.len())?;
                s.serialize_field("matches", ms)?;
                s.end()
            }
        }
    }
}

/// Parse a formula. A bare selector is valid.
pub fn parse(src: &str) -> Result<Formula, CoreError> {
    let mut p = Parser::new(src);
    let expr = p.parse_or()?;
    p.skip_ws();
    if !p.eof() {
        return Err(p.error("trailing input after formula"));
    }
    Ok(expr)
}

/// Parse a selector only (no `count(…)` wrapper).
pub fn parse_selector(src: &str) -> Result<Selector, CoreError> {
    let mut p = Parser::new(src);
    let sel = p.parse_selector()?;
    p.skip_ws();
    if !p.eof() {
        return Err(p.error("trailing input after selector"));
    }
    Ok(sel)
}

/// Parse and evaluate in one step.
pub fn eval_str(root: &XmlNode, src: &str) -> Result<Value, CoreError> {
    eval(root, &parse(src)?)
}

/// Parse a selector and return matches.
pub fn select_str(root: &XmlNode, src: &str) -> Result<Vec<Match>, CoreError> {
    select(root, &parse_selector(src)?)
}

/// Evaluate `formula` against the document element `root`.
pub fn eval(root: &XmlNode, formula: &Formula) -> Result<Value, CoreError> {
    match formula {
        Formula::Literal(Literal::Null) => Ok(Value::Null),
        Formula::Literal(Literal::Bool(b)) => Ok(Value::Bool(*b)),
        Formula::Literal(Literal::Number(n)) => Ok(Value::Number(*n)),
        Formula::Literal(Literal::String(s)) => Ok(Value::String(s.clone())),
        Formula::Selector(sel) => Ok(Value::Nodes(select(root, sel)?)),
        Formula::Call { name, args } => eval_call(root, name, args),
        Formula::Binary { op, left, right } => {
            let l = eval(root, left)?;
            let r = eval(root, right)?;
            match op {
                BinOp::Eq => Ok(Value::Bool(values_eq(&l, &r))),
                BinOp::Ne => Ok(Value::Bool(!values_eq(&l, &r))),
                BinOp::And => Ok(Value::Bool(l.truthy() && r.truthy())),
                BinOp::Or => Ok(Value::Bool(l.truthy() || r.truthy())),
            }
        }
    }
}

/// Run a selector. Empty path (bare `/`) returns the root.
pub fn select(root: &XmlNode, selector: &Selector) -> Result<Vec<Match>, CoreError> {
    let start = vec![Ctx {
        node: root,
        path: NodePath::root(),
    }];
    let found = apply_steps(root, start, &selector.steps)?;
    let mut matches = Vec::new();
    for ctx in found {
        if let Some(attr) = selector.attr.as_deref() {
            if let Some(value) = ctx.node.get_attr(attr) {
                matches.push(Match {
                    path: ctx.path.display(),
                    name: ctx.node.qname(),
                    text: value.to_string(),
                    attr: Some(attr.to_string()),
                });
            }
        } else {
            matches.push(ctx_match(&ctx));
        }
    }
    Ok(matches)
}

fn ctx_match(ctx: &Ctx<'_>) -> Match {
    Match {
        path: ctx.path.display(),
        name: ctx.node.qname(),
        text: ctx.node.text_content(),
        attr: None,
    }
}

fn values_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Nodes(a), Value::Nodes(b)) => a
            .iter()
            .map(|m| m.path.as_str())
            .eq(b.iter().map(|m| m.path.as_str())),
        (Value::Nodes(_), Value::Number(_)) | (Value::Number(_), Value::Nodes(_)) => {
            left.as_number() == right.as_number()
        }
        (Value::Number(_), Value::Bool(_)) | (Value::Bool(_), Value::Number(_)) => {
            left.as_number() == right.as_number()
        }
        _ => left.to_text() == right.to_text(),
    }
}

fn eval_call(root: &XmlNode, name: &str, args: &[Formula]) -> Result<Value, CoreError> {
    let n = name.replace('-', "_");
    match n.as_str() {
        "count" => {
            let nodes = expect_nodes(root, args, "count")?;
            Ok(Value::Number(nodes.len() as i64))
        }
        "exists" => {
            let nodes = expect_nodes(root, args, "exists")?;
            Ok(Value::Bool(!nodes.is_empty()))
        }
        "text" | "string" => {
            let v = one_arg(root, args, "text")?;
            Ok(Value::String(v.to_text()))
        }
        "name" => {
            let nodes = expect_nodes(root, args, "name")?;
            Ok(Value::String(
                nodes.first().map(|m| m.name.clone()).unwrap_or_default(),
            ))
        }
        "local_name" => {
            let nodes = expect_nodes(root, args, "local_name")?;
            let qname = nodes.first().map(|m| m.name.as_str()).unwrap_or("");
            let local = qname.split(':').next_back().unwrap_or(qname);
            Ok(Value::String(local.to_string()))
        }
        "attr" => {
            if args.len() != 2 {
                return Err(CoreError::formula("attr(selector, name) takes 2 arguments"));
            }
            let nodes = match eval(root, &args[0])? {
                Value::Nodes(ms) => ms,
                other => {
                    return Err(CoreError::formula(format!(
                        "attr() first argument must select nodes, got {}",
                        other.kind_name()
                    )));
                }
            };
            let attr_name = eval(root, &args[1])?.to_text();
            let Some(first) = nodes.first() else {
                return Ok(Value::Null);
            };
            let path = NodePath::parse(first.path.trim_start_matches('/'))?;
            let node = path.resolve(root)?;
            match node.get_attr(&attr_name) {
                Some(v) => Ok(Value::String(v.to_string())),
                None => Ok(Value::Null),
            }
        }
        "contains" => {
            if args.len() != 2 {
                return Err(CoreError::formula("contains(a, b) takes 2 arguments"));
            }
            let hay = eval(root, &args[0])?.to_text();
            let needle = eval(root, &args[1])?.to_text();
            Ok(Value::Bool(hay.contains(&needle)))
        }
        "starts_with" => {
            if args.len() != 2 {
                return Err(CoreError::formula("starts_with(a, b) takes 2 arguments"));
            }
            let hay = eval(root, &args[0])?.to_text();
            let prefix = eval(root, &args[1])?.to_text();
            Ok(Value::Bool(hay.starts_with(&prefix)))
        }
        "not" => {
            let v = one_arg(root, args, "not")?;
            Ok(Value::Bool(!v.truthy()))
        }
        "ends_with" => {
            if args.len() != 2 {
                return Err(CoreError::formula("ends_with(a, b) takes 2 arguments"));
            }
            let hay = eval(root, &args[0])?.to_text();
            let suffix = eval(root, &args[1])?.to_text();
            Ok(Value::Bool(hay.ends_with(&suffix)))
        }
        "concat" => {
            if args.is_empty() {
                return Err(CoreError::formula("concat() takes at least 1 argument"));
            }
            let mut out = String::new();
            for arg in args {
                out.push_str(&eval(root, arg)?.to_text());
            }
            Ok(Value::String(out))
        }
        "substring" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CoreError::formula(
                    "substring(s, start[, length]) takes 2 or 3 arguments",
                ));
            }
            let s = eval(root, &args[0])?.to_text();
            let start = eval(root, &args[1])?
                .as_number()
                .ok_or_else(|| CoreError::formula("substring start must be a number"))?;
            let len = if args.len() == 3 {
                Some(
                    eval(root, &args[2])?
                        .as_number()
                        .ok_or_else(|| CoreError::formula("substring length must be a number"))?,
                )
            } else {
                None
            };
            Ok(Value::String(substring_chars(&s, start, len)))
        }
        "substring_before" => {
            if args.len() != 2 {
                return Err(CoreError::formula(
                    "substring_before(s, sep) takes 2 arguments",
                ));
            }
            let s = eval(root, &args[0])?.to_text();
            let sep = eval(root, &args[1])?.to_text();
            Ok(Value::String(match s.split_once(&sep) {
                Some((before, _)) => before.to_string(),
                None => String::new(),
            }))
        }
        "substring_after" => {
            if args.len() != 2 {
                return Err(CoreError::formula(
                    "substring_after(s, sep) takes 2 arguments",
                ));
            }
            let s = eval(root, &args[0])?.to_text();
            let sep = eval(root, &args[1])?.to_text();
            Ok(Value::String(match s.split_once(&sep) {
                Some((_, after)) => after.to_string(),
                None => String::new(),
            }))
        }
        "string_length" => {
            let v = one_arg(root, args, "string_length")?;
            Ok(Value::Number(v.to_text().chars().count() as i64))
        }
        "normalize_space" => {
            let v = one_arg(root, args, "normalize_space")?;
            let norm = v.to_text().split_whitespace().collect::<Vec<_>>().join(" ");
            Ok(Value::String(norm))
        }
        "boolean" => {
            let v = one_arg(root, args, "boolean")?;
            Ok(Value::Bool(v.truthy()))
        }
        "number" => {
            let v = one_arg(root, args, "number")?;
            Ok(Value::Number(coerce_number(&v)))
        }
        other => Err(CoreError::formula(format!("unknown function '{other}'"))),
    }
}

fn substring_chars(s: &str, start: i64, len: Option<i64>) -> String {
    let chars: Vec<char> = s.chars().collect();
    if start < 0 {
        return String::new();
    }
    let start = start as usize;
    if start >= chars.len() {
        return String::new();
    }
    match len {
        None => chars[start..].iter().collect(),
        Some(n) if n <= 0 => String::new(),
        Some(n) => chars[start..].iter().take(n as usize).collect(),
    }
}

fn coerce_number(value: &Value) -> i64 {
    if let Some(n) = value.as_number() {
        return n;
    }
    value.to_text().trim().parse::<i64>().unwrap_or(0)
}

fn one_arg(root: &XmlNode, args: &[Formula], name: &str) -> Result<Value, CoreError> {
    if args.len() != 1 {
        return Err(CoreError::formula(format!("{name}() takes 1 argument")));
    }
    eval(root, &args[0])
}

fn expect_nodes(root: &XmlNode, args: &[Formula], name: &str) -> Result<Vec<Match>, CoreError> {
    match one_arg(root, args, name)? {
        Value::Nodes(ms) => Ok(ms),
        other => Err(CoreError::formula(format!(
            "{name}() expects a selector, got {}",
            other.kind_name()
        ))),
    }
}

impl Value {
    fn kind_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Nodes(_) => "nodes",
        }
    }
}

#[derive(Clone)]
struct Ctx<'a> {
    node: &'a XmlNode,
    path: NodePath,
}

fn apply_steps<'a>(
    root: &'a XmlNode,
    start: Vec<Ctx<'a>>,
    steps: &[Step],
) -> Result<Vec<Ctx<'a>>, CoreError> {
    let mut cur = start;
    for step in steps {
        let mut next = Vec::new();
        for ctx in &cur {
            next.extend(apply_step(root, ctx, step)?);
        }
        cur = unique_by_path(next);
    }
    Ok(cur)
}

fn unique_by_path(ctxs: Vec<Ctx<'_>>) -> Vec<Ctx<'_>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for ctx in ctxs {
        if seen.insert(ctx.path.display()) {
            out.push(ctx);
        }
    }
    out
}

fn apply_step<'a>(
    root: &'a XmlNode,
    ctx: &Ctx<'a>,
    step: &Step,
) -> Result<Vec<Ctx<'a>>, CoreError> {
    let mut raw = match step.test {
        NodeTest::SelfNode => vec![ctx.clone()],
        NodeTest::Parent => parent_ctx(root, ctx),
        NodeTest::Any | NodeTest::Name(_) => match step.axis {
            Axis::Child => child_elements(ctx),
            Axis::Descendant => descendant_or_self(ctx),
        },
    };

    if let NodeTest::Name(local) = &step.test {
        raw.retain(|c| c.node.is_element_with_local_name(local));
    } else if matches!(step.test, NodeTest::Any) {
        raw.retain(|c| c.node.is_element());
    }

    for pred in &step.predicates {
        raw = apply_predicate(raw, pred);
    }
    Ok(raw)
}

fn apply_predicate<'a>(mut nodes: Vec<Ctx<'a>>, pred: &Predicate) -> Vec<Ctx<'a>> {
    match pred {
        Predicate::HasAttr(name) => {
            nodes.retain(|c| c.node.get_attr(name).is_some());
            nodes
        }
        Predicate::AttrEq { name, value } => {
            nodes.retain(|c| c.node.get_attr(name) == Some(value.as_str()));
            nodes
        }
        Predicate::Index(i) => nodes.into_iter().nth(*i).into_iter().collect(),
        Predicate::Last => nodes.into_iter().next_back().into_iter().collect(),
    }
}

fn parent_ctx<'a>(root: &'a XmlNode, ctx: &Ctx<'a>) -> Vec<Ctx<'a>> {
    if ctx.path.steps.is_empty() {
        return Vec::new();
    }
    let parent_path = NodePath {
        steps: ctx.path.steps[..ctx.path.steps.len() - 1].to_vec(),
    };
    match parent_path.resolve(root) {
        Ok(node) => vec![Ctx {
            node,
            path: parent_path,
        }],
        Err(_) => Vec::new(),
    }
}

fn child_elements<'a>(ctx: &Ctx<'a>) -> Vec<Ctx<'a>> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut out = Vec::new();
    for child in ctx.node.children() {
        if !child.is_element() {
            continue;
        }
        let local = child.local_name().to_string();
        let index = match counts.iter_mut().find(|(n, _)| n == &local) {
            Some((_, n)) => {
                let i = *n;
                *n += 1;
                i
            }
            None => {
                counts.push((local.clone(), 1));
                0
            }
        };
        let mut path = ctx.path.clone();
        path.steps.push(PathStep::Name { local, index });
        out.push(Ctx { node: child, path });
    }
    out
}

fn descendant_or_self<'a>(ctx: &Ctx<'a>) -> Vec<Ctx<'a>> {
    let mut out = Vec::new();
    collect_descendants(ctx, &mut out);
    out
}

fn collect_descendants<'a>(ctx: &Ctx<'a>, out: &mut Vec<Ctx<'a>>) {
    if ctx.node.is_element() {
        out.push(ctx.clone());
    }
    for child in child_elements(ctx) {
        collect_descendants(&child, out);
    }
}

// -----------------------------------------------------------------------------
// Parser
// -----------------------------------------------------------------------------

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
        self.skip_peek().is_none()
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn skip_peek(&self) -> Option<char> {
        self.rest().trim_start().chars().next()
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
        if self.rest().starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn error(&self, msg: impl Into<String>) -> CoreError {
        let msg = msg.into();
        let rest = self.rest().chars().take(24).collect::<String>();
        if rest.is_empty() {
            CoreError::formula(format!("{msg} at end of input"))
        } else {
            CoreError::formula(format!("{msg} at position {} near '{rest}'", self.pos))
        }
    }

    fn parse_or(&mut self) -> Result<Formula, CoreError> {
        let mut left = self.parse_and()?;
        loop {
            self.skip_ws();
            if self.eat_keyword("or") {
                let right = self.parse_and()?;
                left = Formula::Binary {
                    op: BinOp::Or,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Formula, CoreError> {
        let mut left = self.parse_compare()?;
        loop {
            self.skip_ws();
            if self.eat_keyword("and") {
                let right = self.parse_compare()?;
                left = Formula::Binary {
                    op: BinOp::And,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_compare(&mut self) -> Result<Formula, CoreError> {
        let left = self.parse_primary()?;
        self.skip_ws();
        let op = if self.eat("!=") {
            Some(BinOp::Ne)
        } else if self.eat("=") {
            Some(BinOp::Eq)
        } else {
            None
        };
        if let Some(op) = op {
            let right = self.parse_primary()?;
            Ok(Formula::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_primary(&mut self) -> Result<Formula, CoreError> {
        self.skip_ws();
        match self.peek() {
            None => Err(self.error("expected formula")),
            Some('(') => {
                self.bump();
                let inner = self.parse_or()?;
                self.skip_ws();
                if !self.eat(")") {
                    return Err(self.error("expected ')'"));
                }
                Ok(inner)
            }
            Some('"' | '\'') => Ok(Formula::Literal(Literal::String(self.parse_string()?))),
            Some(c) if c.is_ascii_digit() || c == '-' => {
                Ok(Formula::Literal(Literal::Number(self.parse_number()?)))
            }
            Some('/' | '.' | '*') => Ok(Formula::Selector(self.parse_selector()?)),
            Some(c) if is_ident_start(c) => {
                let ident = self.peek_ident();
                let after = self.rest()[ident.len()..].trim_start();
                if after.starts_with('(') {
                    let name = self.parse_ident()?;
                    self.skip_ws();
                    if !self.eat("(") {
                        return Err(self.error("expected '('"));
                    }
                    let args = self.parse_args()?;
                    Ok(Formula::Call {
                        name: canonicalize_fn(&name),
                        args,
                    })
                } else if ident == "true" && !continues_selector(after) {
                    self.parse_ident()?;
                    Ok(Formula::Literal(Literal::Bool(true)))
                } else if ident == "false" && !continues_selector(after) {
                    self.parse_ident()?;
                    Ok(Formula::Literal(Literal::Bool(false)))
                } else {
                    Ok(Formula::Selector(self.parse_selector()?))
                }
            }
            Some(_) => Err(self.error("unexpected character")),
        }
    }

    fn parse_args(&mut self) -> Result<Vec<Formula>, CoreError> {
        self.skip_ws();
        if self.eat(")") {
            return Ok(Vec::new());
        }
        let mut args = vec![self.parse_or()?];
        loop {
            self.skip_ws();
            if self.eat(")") {
                break;
            }
            if !self.eat(",") {
                return Err(self.error("expected ',' or ')'"));
            }
            args.push(self.parse_or()?);
        }
        Ok(args)
    }

    fn parse_selector(&mut self) -> Result<Selector, CoreError> {
        self.skip_ws();
        let mut steps = Vec::new();
        let mut attr = None;
        if self.eat("//") {
            if self.eat_attr_mark() {
                steps.push(Step {
                    axis: Axis::Descendant,
                    test: NodeTest::Any,
                    predicates: Vec::new(),
                });
                attr = Some(self.parse_attr_name()?);
            } else {
                steps.push(self.parse_step(Axis::Descendant)?);
            }
        } else if self.eat("/") {
            if self.eat_attr_mark() {
                attr = Some(self.parse_attr_name()?);
            } else if self.at_step_start() {
                steps.push(self.parse_step(Axis::Child)?);
            }
        } else if self.eat_attr_mark() {
            attr = Some(self.parse_attr_name()?);
        } else {
            steps.push(self.parse_step(Axis::Child)?);
        }
        if attr.is_none() {
            loop {
                self.skip_ws();
                if self.eat("//") {
                    if self.eat_attr_mark() {
                        steps.push(Step {
                            axis: Axis::Descendant,
                            test: NodeTest::Any,
                            predicates: Vec::new(),
                        });
                        attr = Some(self.parse_attr_name()?);
                        break;
                    }
                    steps.push(self.parse_step(Axis::Descendant)?);
                } else if self.eat("/") {
                    if self.eat_attr_mark() {
                        attr = Some(self.parse_attr_name()?);
                        break;
                    }
                    steps.push(self.parse_step(Axis::Child)?);
                } else {
                    break;
                }
            }
        }
        Ok(Selector { steps, attr })
    }

    fn eat_attr_mark(&mut self) -> bool {
        self.skip_ws();
        self.eat("@")
    }

    fn parse_attr_name(&mut self) -> Result<String, CoreError> {
        self.parse_qname_full()
    }

    fn at_step_start(&self) -> bool {
        matches!(self.peek(), Some(c) if c == '.' || c == '*' || is_ident_start(c))
    }

    fn parse_step(&mut self, axis: Axis) -> Result<Step, CoreError> {
        self.skip_ws();
        let test = if self.eat("..") {
            NodeTest::Parent
        } else if self.eat(".") {
            NodeTest::SelfNode
        } else if self.eat("*") {
            NodeTest::Any
        } else {
            NodeTest::Name(self.parse_qname()?)
        };
        let mut predicates = Vec::new();
        loop {
            self.skip_ws();
            if !self.eat("[") {
                break;
            }
            predicates.push(self.parse_predicate()?);
            self.skip_ws();
            if !self.eat("]") {
                return Err(self.error("expected ']'"));
            }
        }
        Ok(Step {
            axis,
            test,
            predicates,
        })
    }

    fn parse_predicate(&mut self) -> Result<Predicate, CoreError> {
        self.skip_ws();
        if self.eat("@") {
            let name = self.parse_qname()?;
            self.skip_ws();
            if self.eat("=") {
                self.skip_ws();
                let value = self.parse_string()?;
                return Ok(Predicate::AttrEq { name, value });
            }
            return Ok(Predicate::HasAttr(name));
        }
        if matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            let n = self.parse_number()?;
            if n < 0 {
                return Err(self.error("index must be >= 0"));
            }
            return Ok(Predicate::Index(n as usize));
        }
        if self.peek_ident() == "last" {
            self.parse_ident()?;
            self.skip_ws();
            if !self.eat("(") {
                return Err(self.error("expected '(' after last"));
            }
            self.skip_ws();
            if !self.eat(")") {
                return Err(self.error("expected ')' after last("));
            }
            return Ok(Predicate::Last);
        }
        Err(self.error("expected @attr, @attr=\"value\", last(), or a 0-based index"))
    }

    fn parse_qname(&mut self) -> Result<String, CoreError> {
        let full = self.parse_qname_full()?;
        Ok(full.split(':').next_back().unwrap_or(&full).to_string())
    }

    fn parse_qname_full(&mut self) -> Result<String, CoreError> {
        let first = self.parse_ident()?;
        if self.eat(":") {
            let local = self.parse_ident()?;
            Ok(format!("{first}:{local}"))
        } else {
            Ok(first)
        }
    }

    fn parse_ident(&mut self) -> Result<String, CoreError> {
        self.skip_ws();
        let ident = self.peek_ident().to_string();
        if ident.is_empty() {
            return Err(self.error("expected name"));
        }
        self.pos += ident.len();
        Ok(ident)
    }

    fn peek_ident(&self) -> &str {
        let rest = self.rest();
        let mut chars = rest.char_indices();
        let Some((_, c)) = chars.next() else {
            return "";
        };
        if !is_ident_start(c) {
            return "";
        }
        let end = chars
            .find(|(_, c)| !is_ident_continue(*c))
            .map(|(i, _)| i)
            .unwrap_or(rest.len());
        &rest[..end]
    }

    fn eat_keyword(&mut self, kw: &str) -> bool {
        if self.peek_ident() == kw {
            self.pos += kw.len();
            true
        } else {
            false
        }
    }

    fn parse_number(&mut self) -> Result<i64, CoreError> {
        self.skip_ws();
        let rest = self.rest();
        let neg = rest.starts_with('-');
        let digits = if neg { &rest[1..] } else { rest };
        let n = digits
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(digits.len());
        if n == 0 {
            return Err(self.error("expected number"));
        }
        let token = if neg { &rest[..n + 1] } else { &rest[..n] };
        let parsed = token
            .parse::<i64>()
            .map_err(|_| self.error("number out of range"))?;
        self.pos += token.len();
        Ok(parsed)
    }

    fn parse_string(&mut self) -> Result<String, CoreError> {
        self.skip_ws();
        let quote = match self.bump() {
            Some(q @ ('"' | '\'')) => q,
            _ => return Err(self.error("expected string literal")),
        };
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
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn continues_selector(after: &str) -> bool {
    after.starts_with('/') || after.starts_with('[') || after.starts_with(':')
}

fn canonicalize_fn(name: &str) -> String {
    name.replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml;

    fn root(xml: &str) -> XmlNode {
        xml::parse(xml.as_bytes()).unwrap().root
    }

    fn word_sample() -> XmlNode {
        root(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
              <w:body>
                <w:p><w:r><w:t>The notice period is </w:t></w:r>
                  <w:del w:author="Alex Rivera" w:id="1"><w:r><w:delText>thirty (30)</w:delText></w:r></w:del>
                  <w:ins w:author="Alex Rivera" w:id="2"><w:r><w:t>sixty (60)</w:t></w:r></w:ins>
                  <w:r><w:t> days.</w:t></w:r>
                </w:p>
                <w:p><w:r><w:t>Payment is due on the Closing Date.</w:t></w:r></w:p>
                <w:hyperlink r:id="rId4"><w:r><w:t>link</w:t></w:r></w:hyperlink>
              </w:body>
            </w:document>"#,
        )
    }

    #[test]
    fn count_descendants() {
        let v = eval_str(&word_sample(), "count(//p)").unwrap();
        assert_eq!(v, Value::Number(2));
        assert_eq!(
            eval_str(&word_sample(), "count(//ins)").unwrap(),
            Value::Number(1)
        );
        assert_eq!(
            eval_str(&word_sample(), "count(//del)").unwrap(),
            Value::Number(1)
        );
    }

    #[test]
    fn exists_and_not() {
        let root = word_sample();
        assert_eq!(eval_str(&root, "exists(//ins)").unwrap(), Value::Bool(true));
        assert_eq!(
            eval_str(&root, "not(exists(//commentRangeStart))").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str(&root, "exists(//ins) and exists(//del)").unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn text_and_contains_redline() {
        let root = word_sample();
        let t = eval_str(&root, "text(//p[0])").unwrap();
        let text = t.as_str().unwrap();
        assert!(text.contains("The notice period is"));
        assert!(text.contains("thirty (30)"));
        assert!(text.contains("sixty (60)"));
        assert_eq!(
            eval_str(&root, r#"contains(text(//delText), "thirty")"#).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str(&root, r#"starts_with(text(//p[1]), "Payment")"#).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str(&root, r#"starts-with(text(//p[1]), "Payment")"#).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn attr_and_name() {
        let root = word_sample();
        assert_eq!(
            eval_str(&root, r#"attr(//ins[0], "author")"#).unwrap(),
            Value::String("Alex Rivera".into())
        );
        assert_eq!(
            eval_str(&root, "name(//p[0])").unwrap(),
            Value::String("w:p".into())
        );
        assert_eq!(
            eval_str(&root, "local-name(//p[0])").unwrap(),
            Value::String("p".into())
        );
        assert_eq!(
            eval_str(&root, r#"attr(//ins[0], "missing")"#).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn attr_predicate_rfc5261() {
        let root = word_sample();
        let ms = select_str(&root, r#"//hyperlink[@id="rId4"]"#).unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].text, "link");
        assert!(select_str(&root, r#"//hyperlink[@id="nope"]"#)
            .unwrap()
            .is_empty());
        assert_eq!(select_str(&root, "//ins[@author]").unwrap().len(), 1);
    }

    #[test]
    fn child_path_matches_nodepath() {
        let root = word_sample();
        let ms = select_str(&root, "/body/p[1]").unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].path, "/body[0]/p[1]");
        assert!(ms[0].text.contains("Payment"));
        let same = select_str(&root, "body/p[1]").unwrap();
        assert_eq!(same[0].path, ms[0].path);
    }

    #[test]
    fn descendant_index_is_document_order() {
        let root = word_sample();
        let first = select_str(&root, "//p[0]").unwrap();
        assert!(first[0].text.contains("notice"));
        let second = select_str(&root, "//p[1]").unwrap();
        assert!(second[0].text.contains("Payment"));
    }

    #[test]
    fn wildcard_and_dot() {
        let root = word_sample();
        assert_eq!(
            eval_str(&root, "count(.//*)").unwrap(),
            eval_str(&root, "count(//*)").unwrap()
        );
        assert_eq!(
            eval_str(&root, "name(.)").unwrap(),
            Value::String("w:document".into())
        );
        assert_eq!(
            eval_str(&root, "name(body/p[0]/..)").unwrap(),
            Value::String("w:body".into())
        );
    }

    #[test]
    fn comparison_count_and_text() {
        let root = word_sample();
        assert_eq!(
            eval_str(&root, "count(//p) = 2").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(eval_str(&root, "//p = 2").unwrap(), Value::Bool(true));
        assert_eq!(
            eval_str(
                &root,
                r#"text(//p[1]) = "Payment is due on the Closing Date.""#
            )
            .unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str(&root, "count(//ins) != 0").unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn bare_selector_is_nodes() {
        let v = eval_str(&word_sample(), "//ins").unwrap();
        let ms = v.matches().unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].name, "w:ins");
    }

    #[test]
    fn slash_alone_is_root() {
        let root = word_sample();
        let ms = select_str(&root, "/").unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].path, "/");
        assert_eq!(ms[0].name, "w:document");
    }

    #[test]
    fn parse_errors() {
        assert!(parse("count(").is_err());
        assert!(parse("nope(//p)").is_err() || eval_str(&word_sample(), "nope(//p)").is_err());
        assert!(parse_selector("body/p[x]").is_err());
        assert!(eval_str(&word_sample(), "count(1)").is_err());
    }

    #[test]
    fn json_shape() {
        let v = eval_str(&word_sample(), "count(//ins)").unwrap();
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["kind"], "number");
        assert_eq!(json["value"], 1);
        let v = eval_str(&word_sample(), "//p[0]").unwrap();
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["kind"], "nodes");
        assert_eq!(json["count"], 1);
        assert!(json["matches"][0]["path"]
            .as_str()
            .unwrap()
            .contains("p[0]"));
    }

    #[test]
    fn prefix_in_selector_is_local_name() {
        let root = word_sample();
        assert_eq!(
            eval_str(&root, "count(//w:ins)").unwrap(),
            eval_str(&root, "count(//ins)").unwrap()
        );
    }

    #[test]
    fn attr_step_and_last() {
        let root = word_sample();
        let ms = select_str(&root, "//ins/@author").unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].attr.as_deref(), Some("author"));
        assert_eq!(ms[0].text, "Alex Rivera");
        assert_eq!(
            eval_str(&root, r#"//ins/@author = "Alex Rivera""#).unwrap(),
            Value::Bool(true)
        );
        let last = select_str(&root, "//p[last()]").unwrap();
        assert!(last[0].text.contains("Payment"));
        assert_eq!(
            eval_str(&root, "count(//@author)").unwrap(),
            Value::Number(2)
        );
    }

    #[test]
    fn xpath_string_functions() {
        let root = word_sample();
        assert_eq!(
            eval_str(&root, r#"substring(text(//p[1]), 0, 7)"#).unwrap(),
            Value::String("Payment".into())
        );
        assert_eq!(
            eval_str(&root, r#"substring_before(text(//p[1]), " due")"#).unwrap(),
            Value::String("Payment is".into())
        );
        assert_eq!(
            eval_str(&root, r#"substring-after(text(//p[1]), "due ")"#).unwrap(),
            Value::String("on the Closing Date.".into())
        );
        assert_eq!(
            eval_str(&root, r#"concat("A", "-", "B")"#).unwrap(),
            Value::String("A-B".into())
        );
        assert_eq!(
            eval_str(&root, r#"ends_with(text(//p[1]), "Date.")"#).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str(&root, r#"normalize-space("  a   b  ")"#).unwrap(),
            Value::String("a b".into())
        );
        assert_eq!(
            eval_str(&root, r#"string-length("hi")"#).unwrap(),
            Value::Number(2)
        );
        assert_eq!(
            eval_str(&root, r#"boolean(count(//ins))"#).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str(&root, r#"number("12")"#).unwrap(),
            Value::Number(12)
        );
    }
}
