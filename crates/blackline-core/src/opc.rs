//! Disk-oriented unpack / pack helpers built on [`Package`].

use std::path::Path;

use crate::error::CoreError;
use crate::package::{self, Package, OOXML_EXTENSIONS};

pub use package::OOXML_EXTENSIONS as EXTENSIONS;

/// Unpack an OOXML package into `output_directory`, pretty-printing XML.
///
/// Returns the number of XML parts pretty-printed.
pub fn unpack_archive(input_file: &Path, output_directory: &Path) -> Result<usize, CoreError> {
    if !input_file.exists() {
        return Err(CoreError::invalid(format!(
            "{} does not exist",
            input_file.display()
        )));
    }
    let suffix = extension_of(input_file);
    if !OOXML_EXTENSIONS.contains(&suffix.as_str()) {
        return Err(CoreError::invalid(format!(
            "{} must be a .docx, .pptx, or .xlsx file",
            input_file.display()
        )));
    }
    let pkg = Package::open(input_file)?;
    pkg.extract_to_dir(output_directory)
}

/// Pack a directory tree into an OOXML package at `output_file`.
pub fn pack_dir(input_directory: &Path, output_file: &Path) -> Result<(), CoreError> {
    if !input_directory.is_dir() {
        return Err(CoreError::invalid(format!(
            "{} is not a directory",
            input_directory.display()
        )));
    }
    let suffix = extension_of(output_file);
    if !OOXML_EXTENSIONS.contains(&suffix.as_str()) {
        return Err(CoreError::invalid(format!(
            "{} must be a .docx, .pptx, or .xlsx file",
            output_file.display()
        )));
    }
    let pkg = Package::from_dir(input_directory)?;
    pkg.save(output_file)
}

/// Lowercase extension helper.
pub use package::extension_of;
