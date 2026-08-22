//! A small XML DOM tuned for OOXML editing.
//!
//! Parse with [`parse`], mutate the [`XmlNode`] tree, serialize with
//! [`XmlDocument::to_bytes`] (condensed, for packing) or
//! [`XmlDocument::to_pretty_bytes`] (for unpack / humans).

use quick_xml::events::{BytesDecl, BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::reader::Reader;
use std::io::Cursor;

use crate::error::CoreError;

/// A parsed XML document: optional declaration plus a root node.
#[derive(Debug, Clone)]
pub struct XmlDocument {
    /// `<?xml …?>` declaration, if present.
    pub declaration: Option<XmlDeclaration>,
    /// Document element.
    pub root: XmlNode,
}

/// XML declaration attributes.
#[derive(Debug, Clone)]
pub struct XmlDeclaration {
    /// XML version, almost always `"1.0"`.
    pub version: String,
    /// Encoding, typically `"UTF-8"`.
    pub encoding: Option<String>,
    /// Standalone flag.
    pub standalone: Option<bool>,
}

impl XmlDeclaration {
    /// The declaration Word / Excel / PowerPoint emit on every part.
    pub fn office() -> Self {
        Self {
            version: "1.0".into(),
            encoding: Some("UTF-8".into()),
            standalone: Some(true),
        }
    }
}

/// One node in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlNode {
    /// An element with optional prefix, local name, attributes, and children.
    Element {
        /// Namespace prefix (`Some("w")` for `w:p`).
        prefix: Option<String>,
        /// Local name (`"p"` for `w:p`).
        local_name: String,
        /// Attributes as `(full_name, value)` pairs, in source order.
        attributes: Vec<(String, String)>,
        /// Child nodes.
        children: Vec<XmlNode>,
    },
    /// Character data.
    Text(String),
    /// A CDATA section.
    CData(String),
    /// A comment.
    Comment(String),
}

impl XmlNode {
    /// Build an unprefixed element.
    pub fn element(local_name: impl Into<String>) -> Self {
        XmlNode::Element {
            prefix: None,
            local_name: local_name.into(),
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Build a prefixed element (`w:p`, `a:t`, …).
    pub fn prefixed(prefix: impl Into<String>, local_name: impl Into<String>) -> Self {
        XmlNode::Element {
            prefix: Some(prefix.into()),
            local_name: local_name.into(),
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// WordprocessingML element (`w:`).
    pub fn w(local_name: impl Into<String>) -> Self {
        Self::prefixed("w", local_name)
    }

    /// DrawingML element (`a:`).
    pub fn a(local_name: impl Into<String>) -> Self {
        Self::prefixed("a", local_name)
    }

    /// PresentationML element (`p:`).
    pub fn p(local_name: impl Into<String>) -> Self {
        Self::prefixed("p", local_name)
    }

    /// SpreadsheetML element (unprefixed, default namespace).
    pub fn s(local_name: impl Into<String>) -> Self {
        Self::element(local_name)
    }

    /// Add or replace an attribute. Returns `self` for chaining.
    pub fn with_attr(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.set_attr(&name.into(), &value.into());
        self
    }

    /// Append a child. Returns `self` for chaining.
    pub fn with_child(mut self, child: XmlNode) -> Self {
        if let Some(kids) = self.try_children_mut() {
            kids.push(child);
        }
        self
    }

    /// Append a text child. Returns `self` for chaining.
    pub fn with_text(self, text: impl Into<String>) -> Self {
        self.with_child(XmlNode::Text(text.into()))
    }

    /// Qualified name (`w:p`) or local name if unprefixed.
    pub fn qname(&self) -> String {
        match self {
            XmlNode::Element {
                prefix: Some(p),
                local_name,
                ..
            } => format!("{p}:{local_name}"),
            XmlNode::Element { local_name, .. } => local_name.clone(),
            _ => String::new(),
        }
    }

    /// Local name, or `""` for non-elements.
    pub fn local_name(&self) -> &str {
        match self {
            XmlNode::Element { local_name, .. } => local_name.as_str(),
            _ => "",
        }
    }

    /// True when this is an element.
    pub fn is_element(&self) -> bool {
        matches!(self, XmlNode::Element { .. })
    }

    /// True when this is an element whose local name equals `name`.
    pub fn is_element_with_local_name(&self, name: &str) -> bool {
        matches!(self, XmlNode::Element { local_name, .. } if local_name == name)
    }

    /// Attribute value by full name or local name.
    pub fn get_attr(&self, name: &str) -> Option<&str> {
        match self {
            XmlNode::Element { attributes, .. } => {
                for (k, v) in attributes {
                    if k == name {
                        return Some(v.as_str());
                    }
                    if let Some((_, local)) = k.split_once(':') {
                        if local == name {
                            return Some(v.as_str());
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Attribute value by exact full name only.
    pub fn get_attr_exact(&self, name: &str) -> Option<&str> {
        match self {
            XmlNode::Element { attributes, .. } => attributes
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }

    /// Change this element's qualified name. Prefix is taken from `qname`
    /// when it contains a colon (`w:p`); otherwise the element is unprefixed.
    pub fn set_qname(&mut self, qname: &str) {
        if let XmlNode::Element {
            prefix, local_name, ..
        } = self
        {
            if let Some((p, local)) = qname.split_once(':') {
                *prefix = Some(p.to_string());
                *local_name = local.to_string();
            } else {
                *prefix = None;
                *local_name = qname.to_string();
            }
        }
    }

    /// Set or insert an attribute.
    pub fn set_attr(&mut self, name: &str, value: &str) {
        if let XmlNode::Element { attributes, .. } = self {
            for (k, v) in attributes.iter_mut() {
                if k == name {
                    *v = value.to_string();
                    return;
                }
            }
            attributes.push((name.to_string(), value.to_string()));
        }
    }

    /// Remove an attribute by exact name.
    pub fn remove_attr(&mut self, name: &str) {
        if let XmlNode::Element { attributes, .. } = self {
            attributes.retain(|(k, _)| k != name);
        }
    }

    /// Child slice. Empty for non-elements.
    pub fn children(&self) -> &[XmlNode] {
        match self {
            XmlNode::Element { children, .. } => children,
            _ => &[],
        }
    }

    /// Mutable children. Panics on non-elements.
    ///
    /// # Panics
    /// Panics when called on a non-element. Use [`XmlNode::try_children_mut`]
    /// when the kind is not known.
    #[track_caller]
    pub fn children_mut(&mut self) -> &mut Vec<XmlNode> {
        match self {
            XmlNode::Element { children, .. } => children,
            _ => panic!("children_mut called on non-element"),
        }
    }

    /// Mutable children, or `None` for non-elements.
    pub fn try_children_mut(&mut self) -> Option<&mut Vec<XmlNode>> {
        match self {
            XmlNode::Element { children, .. } => Some(children),
            _ => None,
        }
    }

    /// Concatenated descendant text (and CDATA).
    pub fn text_content(&self) -> String {
        match self {
            XmlNode::Text(t) | XmlNode::CData(t) => t.clone(),
            XmlNode::Element { children, .. } => {
                children.iter().map(|c| c.text_content()).collect()
            }
            _ => String::new(),
        }
    }

    /// Replace this element's children with a single text node.
    /// Adds `xml:space="preserve"` when the text has leading/trailing
    /// whitespace — required for OOXML `w:t` / `a:t`.
    pub fn set_text(&mut self, text: &str) {
        if let XmlNode::Element { children, .. } = self {
            *children = vec![XmlNode::Text(text.to_string())];
        }
        if text.starts_with(|c: char| c.is_whitespace())
            || text.ends_with(|c: char| c.is_whitespace())
        {
            self.set_attr("xml:space", "preserve");
        }
    }

    /// All descendant elements with the given local name, document order.
    pub fn find_all(&self, local_name: &str) -> Vec<&XmlNode> {
        let mut results = Vec::new();
        self.find_all_recursive(local_name, &mut results);
        results
    }

    fn find_all_recursive<'a>(&'a self, local_name: &str, results: &mut Vec<&'a XmlNode>) {
        if let XmlNode::Element {
            local_name: ln,
            children,
            ..
        } = self
        {
            if ln == local_name {
                results.push(self);
            }
            for child in children {
                child.find_all_recursive(local_name, results);
            }
        }
    }

    /// Depth-first mutable visit of every node (including `self`).
    pub fn walk_mut<F>(&mut self, f: &mut F)
    where
        F: FnMut(&mut XmlNode),
    {
        f(self);
        if let Some(children) = self.try_children_mut() {
            for child in children.iter_mut() {
                child.walk_mut(f);
            }
        }
    }

    /// Depth-first immutable visit of every node (including `self`).
    pub fn walk<F>(&self, f: &mut F)
    where
        F: FnMut(&XmlNode),
    {
        f(self);
        for child in self.children() {
            child.walk(f);
        }
    }

    /// First direct child element with this local name.
    pub fn find_child(&self, local_name: &str) -> Option<&XmlNode> {
        self.children()
            .iter()
            .find(|c| c.is_element_with_local_name(local_name))
    }

    /// First direct child element with this local name, mutable.
    pub fn find_child_mut(&mut self, local_name: &str) -> Option<&mut XmlNode> {
        self.try_children_mut()?
            .iter_mut()
            .find(|c| c.is_element_with_local_name(local_name))
    }

    /// The `n`th direct child element with this local name (0-based).
    pub fn child_named(&self, local_name: &str, n: usize) -> Option<&XmlNode> {
        self.children()
            .iter()
            .filter(|c| c.is_element_with_local_name(local_name))
            .nth(n)
    }

    /// Mutable `n`th direct child element with this local name (0-based).
    pub fn child_named_mut(&mut self, local_name: &str, n: usize) -> Option<&mut XmlNode> {
        self.try_children_mut()?
            .iter_mut()
            .filter(|c| c.is_element_with_local_name(local_name))
            .nth(n)
    }

    /// Serialize this node (no declaration) to a string.
    pub fn to_xml_string(&self) -> String {
        let mut out = String::new();
        serialize_node(self, &mut out, false, "", 0);
        out
    }
}

/// Parse bytes into an [`XmlDocument`].
pub fn parse(data: &[u8]) -> Result<XmlDocument, CoreError> {
    parse_named(data, "<memory>")
}

/// Parse bytes, tagging errors with `part`.
pub fn parse_named(data: &[u8], part: &str) -> Result<XmlDocument, CoreError> {
    let mut reader = Reader::from_reader(Cursor::new(data));
    reader.config_mut().trim_text_start = false;
    reader.config_mut().trim_text_end = false;

    let mut declaration = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Decl(decl)) => {
                declaration = Some(parse_declaration(&decl));
            }
            Ok(Event::Start(ref e)) => {
                let root = parse_element(&mut reader, e, &mut Vec::new())
                    .map_err(|m| CoreError::xml(part, m))?;
                return Ok(XmlDocument { declaration, root });
            }
            Ok(Event::Empty(ref e)) => {
                let root = parse_empty_element(e);
                return Ok(XmlDocument { declaration, root });
            }
            Ok(Event::Eof) => {
                return Err(CoreError::xml(part, "unexpected EOF before root element"));
            }
            Ok(Event::Comment(_)) | Ok(Event::PI(_)) | Ok(Event::Text(_)) => {}
            Err(e) => return Err(CoreError::xml(part, format!("XML parse error: {e}"))),
            _ => {}
        }
        buf.clear();
    }
}

fn parse_declaration(decl: &BytesDecl) -> XmlDeclaration {
    let version = decl
        .version()
        .map(|v| String::from_utf8_lossy(&v).to_string())
        .unwrap_or_else(|_| "1.0".into());
    let encoding = decl
        .encoding()
        .and_then(|r| r.ok())
        .map(|e| String::from_utf8_lossy(&e).to_string());
    let standalone = decl.standalone().and_then(|r| r.ok()).map(|s| {
        let s = String::from_utf8_lossy(&s);
        s == "yes"
    });
    XmlDeclaration {
        version,
        encoding,
        standalone,
    }
}

fn parse_element(
    reader: &mut Reader<Cursor<&[u8]>>,
    start: &BytesStart,
    buf: &mut Vec<u8>,
) -> Result<XmlNode, String> {
    let (prefix, local_name) = extract_name(start.name());
    let attributes = extract_attributes(start);
    let mut children = Vec::new();

    loop {
        buf.clear();
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                children.push(parse_element(reader, e, &mut Vec::new())?);
            }
            Ok(Event::Empty(ref e)) => {
                children.push(parse_empty_element(e));
            }
            Ok(Event::End(_)) => break,
            Ok(Event::Text(ref e)) => {
                // quick-xml >= 0.37 replaced `BytesText::unescape` with
                // `xml10_content`, which decodes bytes and normalizes EOLs but
                // does not resolve entities. The write path escapes via
                // `xml_escape_text`, so both directions must stay in step.
                let decoded = e
                    .xml10_content()
                    .map_err(|e| format!("text decode error: {e}"))?;
                let text = quick_xml::escape::unescape(&decoded)
                    .map_err(|e| format!("text unescape error: {e}"))?;
                push_text(&mut children, &text);
            }
            Ok(Event::GeneralRef(ref e)) => {
                // quick-xml >= 0.37 emits entity references as their own events
                // rather than inlining them into the adjacent Text event. Without
                // this arm the catch-all below silently DROPS every `&amp;` and
                // `&#38;` in the document.
                let name = e
                    .decode()
                    .map_err(|e| format!("entity decode error: {e}"))?;
                let resolved = quick_xml::escape::unescape(&format!("&{name};"))
                    .map_err(|e| format!("unresolved entity &{name};: {e}"))?
                    .to_string();
                push_text(&mut children, &resolved);
            }
            Ok(Event::CData(ref e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();
                children.push(XmlNode::CData(text));
            }
            Ok(Event::Comment(ref e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();
                children.push(XmlNode::Comment(text));
            }
            Ok(Event::Eof) => return Err("unexpected EOF inside element".into()),
            Err(e) => return Err(format!("XML parse error: {e}")),
            _ => {}
        }
    }

    Ok(XmlNode::Element {
        prefix,
        local_name,
        attributes,
        children,
    })
}

fn parse_empty_element(start: &BytesStart) -> XmlNode {
    let (prefix, local_name) = extract_name(start.name());
    let attributes = extract_attributes(start);
    XmlNode::Element {
        prefix,
        local_name,
        attributes,
        children: Vec::new(),
    }
}

fn extract_name(qname: QName) -> (Option<String>, String) {
    let full = String::from_utf8_lossy(qname.as_ref()).to_string();
    if let Some((prefix, local)) = full.split_once(':') {
        (Some(prefix.to_string()), local.to_string())
    } else {
        (None, full)
    }
}

fn extract_attributes(start: &BytesStart) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    for attr in start.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let value = String::from_utf8_lossy(&attr.value).to_string();
        attrs.push((key, value));
    }
    attrs
}

impl XmlDocument {
    /// Wrap a root element with the standard Office declaration.
    pub fn new(root: XmlNode) -> Self {
        Self {
            declaration: Some(XmlDeclaration::office()),
            root,
        }
    }

    /// Condensed serialization (no pretty-print whitespace).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        self.serialize_declaration(&mut out);
        serialize_node(&self.root, &mut out, false, "", 0);
        out.into_bytes()
    }

    /// Pretty-printed serialization.
    pub fn to_pretty_bytes(&self, indent: &str) -> Vec<u8> {
        let mut out = String::new();
        self.serialize_declaration(&mut out);
        if self.declaration.is_some() {
            out.push('\n');
        }
        serialize_node(&self.root, &mut out, true, indent, 0);
        out.into_bytes()
    }

    fn serialize_declaration(&self, out: &mut String) {
        if let Some(ref decl) = self.declaration {
            out.push_str("<?xml version=\"");
            out.push_str(&decl.version);
            out.push('"');
            if let Some(ref enc) = decl.encoding {
                out.push_str(" encoding=\"");
                out.push_str(enc);
                out.push('"');
            }
            if let Some(standalone) = decl.standalone {
                out.push_str(" standalone=\"");
                out.push_str(if standalone { "yes" } else { "no" });
                out.push('"');
            }
            out.push_str("?>");
        }
    }
}

fn is_text_element(name: &str) -> bool {
    name.ends_with(":t") || name == "t" || name.ends_with(":delText") || name == "delText"
}

fn serialize_node(node: &XmlNode, out: &mut String, pretty: bool, indent: &str, depth: usize) {
    match node {
        XmlNode::Element {
            prefix,
            local_name,
            attributes,
            children,
        } => {
            let full_name = match prefix {
                Some(p) => format!("{p}:{local_name}"),
                None => local_name.clone(),
            };
            let is_text = is_text_element(&full_name);

            if pretty && !is_text {
                for _ in 0..depth {
                    out.push_str(indent);
                }
            }

            out.push('<');
            out.push_str(&full_name);
            for (k, v) in attributes {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                out.push_str(&xml_escape_attr(v));
                out.push('"');
            }

            if children.is_empty() {
                out.push_str("/>");
                if pretty && !is_text {
                    out.push('\n');
                }
                return;
            }

            out.push('>');

            if is_text {
                for child in children {
                    serialize_node(child, out, false, "", 0);
                }
            } else {
                let has_element_children = children.iter().any(|c| c.is_element());
                if pretty && has_element_children {
                    out.push('\n');
                }
                for child in children {
                    if pretty && has_element_children {
                        serialize_node(child, out, true, indent, depth + 1);
                    } else {
                        serialize_node(child, out, false, "", 0);
                    }
                }
                if pretty && has_element_children {
                    for _ in 0..depth {
                        out.push_str(indent);
                    }
                }
            }

            out.push_str("</");
            out.push_str(&full_name);
            out.push('>');
            if pretty && !is_text {
                out.push('\n');
            }
        }
        XmlNode::Text(text) => out.push_str(&xml_escape_text(text)),
        XmlNode::CData(text) => {
            out.push_str("<![CDATA[");
            out.push_str(text);
            out.push_str("]]>");
        }
        XmlNode::Comment(text) => {
            if pretty {
                for _ in 0..depth {
                    out.push_str(indent);
                }
            }
            out.push_str("<!--");
            out.push_str(text);
            out.push_str("-->");
            if pretty {
                out.push('\n');
            }
        }
    }
}

/// Appends text to `children`, merging into a trailing [`XmlNode::Text`]
/// when present. quick-xml splits a single run of character data into
/// several events (text, entity, text, ...), so without merging one
/// logical text run would become several sibling nodes.
fn push_text(children: &mut Vec<XmlNode>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(XmlNode::Text(existing)) = children.last_mut() {
        existing.push_str(text);
    } else {
        children.push(XmlNode::Text(text.to_string()));
    }
}

fn xml_escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Remove whitespace-only text nodes and comments outside text elements.
pub fn condense(doc: &mut XmlDocument) {
    condense_node(&mut doc.root);
}

fn condense_node(node: &mut XmlNode) {
    if let XmlNode::Element {
        local_name,
        prefix,
        children,
        ..
    } = node
    {
        let full = match prefix {
            Some(p) => format!("{p}:{local_name}"),
            None => local_name.clone(),
        };

        if !is_text_element(&full) {
            children.retain(|child| match child {
                XmlNode::Text(t) if t.trim().is_empty() => false,
                XmlNode::Comment(_) => false,
                _ => true,
            });
        }

        for child in children.iter_mut() {
            condense_node(child);
        }
    }
}

/// Highest numeric `w:id` / `id` attribute under `node`. Used to allocate
/// fresh revision / comment / bookmark ids.
pub fn max_numeric_id(node: &XmlNode) -> usize {
    let mut max = 0usize;
    node.walk(&mut |n| {
        if let Some(v) = n.get_attr("id") {
            if let Ok(id) = v.parse::<usize>() {
                if id > max {
                    max = id;
                }
            }
        }
    });
    max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_element() {
        let xml = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><root><child attr=\"val\">text</child></root>";
        let doc = parse(xml).unwrap();
        assert_eq!(doc.root.local_name(), "root");
        assert_eq!(doc.root.children().len(), 1);
        assert_eq!(doc.root.children()[0].local_name(), "child");
        assert_eq!(doc.root.children()[0].get_attr("attr"), Some("val"));
        assert_eq!(doc.root.children()[0].text_content(), "text");
    }

    #[test]
    fn parse_with_namespaces() {
        let xml = b"<?xml version=\"1.0\"?><w:document xmlns:w=\"http://example.com\"><w:body/></w:document>";
        let doc = parse(xml).unwrap();
        assert_eq!(doc.root.local_name(), "document");
        assert!(doc.root.find_child("body").is_some());
    }

    #[test]
    fn roundtrip_preserves_content() {
        let xml =
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><root><a>hello</a><b>world</b></root>";
        let doc = parse(xml).unwrap();
        let output = doc.to_bytes();
        let doc2 = parse(&output).unwrap();
        assert_eq!(doc2.root.text_content(), "helloworld");
    }

    #[test]
    fn builder_chain() {
        let n =
            XmlNode::w("p").with_child(XmlNode::w("r").with_child(XmlNode::w("t").with_text("hi")));
        assert_eq!(n.text_content(), "hi");
        assert_eq!(n.qname(), "w:p");
    }

    #[test]
    fn set_text_preserves_space() {
        let mut t = XmlNode::w("t");
        t.set_text("  padded  ");
        assert_eq!(t.get_attr("xml:space"), Some("preserve"));
        assert_eq!(t.text_content(), "  padded  ");
    }

    #[test]
    fn find_all_descendants() {
        let xml = b"<?xml version=\"1.0\"?><root><p>a</p><div><p>b</p></div></root>";
        let doc = parse(xml).unwrap();
        assert_eq!(doc.root.find_all("p").len(), 2);
    }

    #[test]
    fn max_id_scans_tree() {
        let xml = b"<root><ins id=\"3\"/><del id=\"7\"/></root>";
        let doc = parse(xml).unwrap();
        assert_eq!(max_numeric_id(&doc.root), 7);
    }

    #[test]
    fn parse_empty_is_error() {
        assert!(parse(b"").is_err());
    }

    #[test]
    fn text_entities_are_unescaped_on_parse() {
        // Regression: quick-xml 0.37 replaced `BytesText::unescape` with
        // `xml10_content`, which decodes but does NOT resolve entities.
        // Swapping them naively leaves "&amp;" literal in document text.
        let xml = b"<root><t>Smith &amp; Wesson &lt;draft&gt;</t></root>";
        let doc = parse(xml).unwrap();
        assert_eq!(
            doc.root.find_child("t").unwrap().text_content(),
            "Smith & Wesson <draft>"
        );
    }

    #[test]
    fn text_entities_round_trip_through_serialize() {
        // Parse unescapes, serialize re-escapes. The byte output must be
        // stable across a parse/serialize cycle or repeated edits corrupt text.
        let xml = b"<?xml version=\"1.0\"?><root><t>a &amp; b &lt; c</t></root>";
        let doc = parse(xml).unwrap();
        let out = doc.to_bytes();
        let again = parse(&out).unwrap();
        assert_eq!(
            again.root.find_child("t").unwrap().text_content(),
            "a & b < c"
        );
        assert_eq!(doc.to_bytes(), again.to_bytes());
    }
}
