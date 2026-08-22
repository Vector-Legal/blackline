//! In-memory OPC package: a ZIP of named parts plus helpers for
//! relationships and content types.
//!
//! Untouched parts keep their original bytes. Only parts that are replaced
//! (or newly added) are re-serialized on save.

use std::io::{Cursor, Read, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::ctypes::ContentTypes;
use crate::error::CoreError;
use crate::rels::{self, Relationships};
use crate::xml::{self, XmlDocument};

/// File extensions recognized as OOXML packages.
pub const OOXML_EXTENSIONS: &[&str] = &["docx", "pptx", "xlsx"];

/// An Open Packaging Conventions archive held in memory.
#[derive(Debug, Clone)]
pub struct Package {
    /// `(name, bytes)` in ZIP order. Names use forward slashes, no leading `/`.
    parts: Vec<(String, Vec<u8>)>,
}

impl Package {
    /// An empty package.
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }

    /// Open a package from a filesystem path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| CoreError::io(path, e))?;
        Self::from_bytes(&bytes)
    }

    /// Open a package from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CoreError> {
        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor).map_err(|e| CoreError::Zip(e.to_string()))?;
        let mut parts = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| CoreError::Zip(e.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().replace('\\', "/");
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|e| CoreError::Zip(e.to_string()))?;
            parts.push((name, data));
        }
        Ok(Self { parts })
    }

    /// Number of parts.
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    /// True when the package has no parts.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Iterate `(name, bytes)` in package order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.parts.iter().map(|(n, b)| (n.as_str(), b.as_slice()))
    }

    /// All part names, in package order.
    pub fn part_names(&self) -> Vec<&str> {
        self.parts.iter().map(|(n, _)| n.as_str()).collect()
    }

    /// True when a part exists.
    pub fn has_part(&self, name: &str) -> bool {
        let name = normalize_name(name);
        self.parts.iter().any(|(n, _)| n == &name)
    }

    /// Borrow a part's bytes.
    pub fn part(&self, name: &str) -> Result<&[u8], CoreError> {
        let name = normalize_name(name);
        self.parts
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, b)| b.as_slice())
            .ok_or(CoreError::MissingPart(name))
    }

    /// Parse a part as XML.
    pub fn part_xml(&self, name: &str) -> Result<XmlDocument, CoreError> {
        let bytes = self.part(name)?;
        xml::parse_named(bytes, name)
    }

    /// Replace or insert a part. Existing position is preserved on replace.
    pub fn set_part(&mut self, name: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        let name = normalize_name(&name.into());
        let bytes = bytes.into();
        if let Some((_, slot)) = self.parts.iter_mut().find(|(n, _)| n == &name) {
            *slot = bytes;
            return;
        }
        self.parts.push((name, bytes));
    }

    /// Replace a part with serialized XML.
    pub fn set_part_xml(&mut self, name: &str, doc: &XmlDocument) {
        self.set_part(name, doc.to_bytes());
    }

    /// Remove a part. Returns the old bytes.
    pub fn remove_part(&mut self, name: &str) -> Option<Vec<u8>> {
        let name = normalize_name(name);
        let pos = self.parts.iter().position(|(n, _)| n == &name)?;
        Some(self.parts.remove(pos).1)
    }

    /// Package-level relationships (`_rels/.rels`).
    pub fn package_rels(&self) -> Result<Relationships, CoreError> {
        self.rels_for("")
    }

    /// Relationships belonging to `part` (empty string = package root).
    pub fn rels_for(&self, part: &str) -> Result<Relationships, CoreError> {
        let path = rels::rels_path_for(part);
        match self.part(&path) {
            Ok(bytes) => Relationships::parse(bytes),
            Err(CoreError::MissingPart(_)) => Ok(Relationships::default()),
            Err(e) => Err(e),
        }
    }

    /// Write relationships for `part`.
    pub fn set_rels_for(&mut self, part: &str, rels: &Relationships) {
        self.set_part(rels::rels_path_for(part), rels.to_bytes());
    }

    /// Parse `[Content_Types].xml`, or return office defaults if missing.
    pub fn content_types(&self) -> Result<ContentTypes, CoreError> {
        match self.part("[Content_Types].xml") {
            Ok(bytes) => ContentTypes::parse(bytes),
            Err(CoreError::MissingPart(_)) => Ok(ContentTypes::office_defaults()),
            Err(e) => Err(e),
        }
    }

    /// Write `[Content_Types].xml`.
    pub fn set_content_types(&mut self, ct: &ContentTypes) {
        self.set_part("[Content_Types].xml", ct.to_bytes());
    }

    /// Ensure a part exists with the given content type override (when
    /// `content_type` is `Some`).
    pub fn add_part(
        &mut self,
        name: &str,
        bytes: impl Into<Vec<u8>>,
        content_type: Option<&str>,
    ) -> Result<(), CoreError> {
        self.set_part(name, bytes);
        if let Some(ct) = content_type {
            let mut map = self.content_types()?;
            map.ensure_override(name, ct);
            self.set_content_types(&map);
        }
        Ok(())
    }

    /// Resolve the main document part from package relationships.
    pub fn main_document_part(&self) -> Result<String, CoreError> {
        let rels = self.package_rels()?;
        let rel = rels
            .by_type(crate::ns::rel::OFFICE_DOCUMENT)
            .ok_or_else(|| CoreError::invalid("package has no officeDocument relationship"))?;
        Ok(rels::resolve_target("", &rel.target))
    }

    /// Write the package to a path. Parent directories are created.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
            }
        }
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes).map_err(|e| CoreError::io(path, e))
    }

    /// Serialize the package to ZIP bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, CoreError> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in &self.parts {
                zip.start_file(name.as_str(), options)
                    .map_err(|e| CoreError::Zip(e.to_string()))?;
                zip.write_all(data)
                    .map_err(|e| CoreError::Zip(e.to_string()))?;
            }
            zip.finish().map_err(|e| CoreError::Zip(e.to_string()))?;
        }
        Ok(buf)
    }

    /// Unpack every part into `dir`, pretty-printing XML / `.rels`.
    ///
    /// Returns the number of XML parts pretty-printed.
    pub fn extract_to_dir(&self, dir: impl AsRef<Path>) -> Result<usize, CoreError> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
        let mut xml_count = 0;
        for (name, data) in &self.parts {
            let dest = dir.join(name);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
            }
            let out = if is_xml_part(name) {
                match xml::parse_named(data, name) {
                    Ok(doc) => {
                        xml_count += 1;
                        doc.to_pretty_bytes("  ")
                    }
                    Err(_) => data.clone(),
                }
            } else {
                data.clone()
            };
            std::fs::write(&dest, out).map_err(|e| CoreError::io(&dest, e))?;
        }
        Ok(xml_count)
    }

    /// Build a package from a directory tree (inverse of [`Package::extract_to_dir`]).
    /// XML parts are condensed.
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, CoreError> {
        let dir = dir.as_ref();
        if !dir.is_dir() {
            return Err(CoreError::invalid(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        let mut pkg = Package::new();
        collect_dir(dir, dir, &mut pkg)?;
        Ok(pkg)
    }
}

impl Default for Package {
    fn default() -> Self {
        Self::new()
    }
}

fn collect_dir(root: &Path, dir: &Path, pkg: &mut Package) -> Result<(), CoreError> {
    let entries = std::fs::read_dir(dir).map_err(|e| CoreError::io(dir, e))?;
    let mut names: Vec<_> = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CoreError::io(dir, e))?;
    names.sort_by_key(|e| e.file_name());
    for entry in names {
        let path = entry.path();
        if path.is_dir() {
            collect_dir(root, &path, pkg)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| CoreError::invalid("path not under root"))?
            .to_string_lossy()
            .replace('\\', "/");
        let mut data = std::fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
        if is_xml_part(&rel) {
            if let Ok(mut doc) = xml::parse_named(&data, &rel) {
                xml::condense(&mut doc);
                data = doc.to_bytes();
            }
        }
        pkg.set_part(rel, data);
    }
    Ok(())
}

fn normalize_name(name: &str) -> String {
    name.trim_start_matches('/').replace('\\', "/")
}

fn is_xml_part(name: &str) -> bool {
    name.ends_with(".xml") || name.ends_with(".rels")
}

/// Lowercase extension of `path`, or `""`.
pub fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bytes() {
        let mut pkg = Package::new();
        pkg.set_part("hello.txt", b"world".to_vec());
        pkg.set_part("a/b.xml", b"<root/>".to_vec());
        let bytes = pkg.to_bytes().unwrap();
        let again = Package::from_bytes(&bytes).unwrap();
        assert_eq!(again.part("hello.txt").unwrap(), b"world");
        assert_eq!(again.part("a/b.xml").unwrap(), b"<root/>");
        assert_eq!(again.len(), 2);
    }

    #[test]
    fn replace_preserves_order() {
        let mut pkg = Package::new();
        pkg.set_part("a", b"1".to_vec());
        pkg.set_part("b", b"2".to_vec());
        pkg.set_part("a", b"3".to_vec());
        assert_eq!(pkg.part_names(), vec!["a", "b"]);
        assert_eq!(pkg.part("a").unwrap(), b"3");
    }

    #[test]
    fn dir_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut pkg = Package::new();
        pkg.set_part(
            "[Content_Types].xml",
            b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"
                .to_vec(),
        );
        pkg.set_part("word/document.xml", b"<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p/></w:body></w:document>".to_vec());
        pkg.extract_to_dir(dir.path()).unwrap();
        let again = Package::from_dir(dir.path()).unwrap();
        assert!(again.has_part("word/document.xml"));
        assert!(again.has_part("[Content_Types].xml"));
    }
}
