// ABOUTME: Virtual filesystem layer dispatching archive formats to their readers.
// ABOUTME: Exposes the reader factory used by both the UI and the decode workers.
use std::path::Path;

pub mod archive;
pub mod ordering;
pub mod pdf;
pub mod rar;
pub mod zip;

pub use archive::{ArchiveError, ArchiveReader, build_pages};
pub use ordering::{is_hidden_metadata_path, is_page_image_path, sort_natural};
pub use pdf::PdfArchiveReader;
pub use rar::RarArchiveReader;
pub use zip::ZipArchiveReader;

/// Builds the archive reader matching a path's extension. Centralizes the
/// dispatch so the UI and the decode workers resolve readers identically.
pub fn reader_for_path(path: &Path) -> Result<Box<dyn ArchiveReader>, ArchiveError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" | "zip" => Ok(Box::new(ZipArchiveReader::new(path))),
        "cbr" | "rar" => Ok(Box::new(RarArchiveReader::new(path))),
        "pdf" => Ok(Box::new(PdfArchiveReader::new(path))),
        _ => Err(ArchiveError::UnsupportedFormat(path.display().to_string())),
    }
}

/// Reads a single page's raw bytes by entry path. Decode workers call this so
/// archive decompression stays off the GUI thread.
pub fn read_page_bytes(archive_path: &Path, page_path: &str) -> Result<Vec<u8>, ArchiveError> {
    reader_for_path(archive_path)?.read_page(page_path)
}
