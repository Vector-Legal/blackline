//! Office Open XML namespace URIs and relationship types.
//!
//! These are the ECMA-376 / ISO 29500 identifiers. Format crates use them
//! when writing parts and relationships so the strings live in one place.

/// WordprocessingML main namespace (`w:`).
pub const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
/// DrawingML main namespace (`a:`).
pub const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
/// SpreadsheetML main namespace (`s:` on some parts; typically default).
pub const S: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
/// PresentationML main namespace (`p:`).
pub const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
/// Office document relationships (`r:`).
pub const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// Package relationships.
pub const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
/// Content types.
pub const CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
/// Markup compatibility (`mc:`).
pub const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
/// Dublin Core.
pub const DC: &str = "http://purl.org/dc/elements/1.1/";
/// Dublin Core terms.
pub const DCTERMS: &str = "http://purl.org/dc/terms/";
/// Core properties.
pub const CP: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
/// Extended properties.
pub const EP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties";
/// XML Schema instance.
pub const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
/// XML namespace (for `xml:space`).
pub const XML: &str = "http://www.w3.org/XML/1998/namespace";

/// Relationship type URIs used in `.rels` parts.
pub mod rel {
    /// Office document (the main part pointed at from the package).
    pub const OFFICE_DOCUMENT: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
    /// Core file properties.
    pub const CORE_PROPERTIES: &str =
        "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
    /// Extended file properties.
    pub const EXTENDED_PROPERTIES: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";
    /// Styles part.
    pub const STYLES: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
    /// Settings part.
    pub const SETTINGS: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    /// Theme part.
    pub const THEME: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
    /// Font table.
    pub const FONT_TABLE: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable";
    /// Numbering.
    pub const NUMBERING: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
    /// Comments.
    pub const COMMENTS: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
    /// Footnotes.
    pub const FOOTNOTES: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes";
    /// Endnotes.
    pub const ENDNOTES: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes";
    /// Header.
    pub const HEADER: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
    /// Footer.
    pub const FOOTER: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer";
    /// Hyperlink.
    pub const HYPERLINK: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
    /// Image.
    pub const IMAGE: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
    /// Worksheet.
    pub const WORKSHEET: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
    /// Shared strings.
    pub const SHARED_STRINGS: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";
    /// Slide.
    pub const SLIDE: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
    /// Slide layout.
    pub const SLIDE_LAYOUT: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
    /// Slide master.
    pub const SLIDE_MASTER: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
    /// Notes slide.
    pub const NOTES_SLIDE: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide";
    /// Notes master.
    pub const NOTES_MASTER: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster";
}

/// Content type strings used in `[Content_Types].xml`.
pub mod content {
    /// XML.
    pub const XML: &str = "application/xml";
    /// Relationships.
    pub const RELS: &str = "application/vnd.openxmlformats-package.relationships+xml";
    /// Word main document.
    pub const DOCUMENT: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
    /// Word styles.
    pub const STYLES: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
    /// Word settings.
    pub const SETTINGS: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
    /// Word comments.
    pub const COMMENTS: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml";
    /// Word footnotes.
    pub const FOOTNOTES: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml";
    /// Theme.
    pub const THEME: &str = "application/vnd.openxmlformats-officedocument.theme+xml";
    /// Core properties.
    pub const CORE_PROPS: &str = "application/vnd.openxmlformats-package.core-properties+xml";
    /// Extended properties.
    pub const APP_PROPS: &str =
        "application/vnd.openxmlformats-officedocument.extended-properties+xml";
    /// Spreadsheet workbook.
    pub const WORKBOOK: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
    /// Worksheet.
    pub const WORKSHEET: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
    /// Shared strings.
    pub const SHARED_STRINGS: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";
    /// Spreadsheet styles.
    pub const SHEET_STYLES: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
    /// Presentation.
    pub const PRESENTATION: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
    /// Slide.
    pub const SLIDE: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
    /// Slide layout.
    pub const SLIDE_LAYOUT: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml";
    /// Slide master.
    pub const SLIDE_MASTER: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml";
}
