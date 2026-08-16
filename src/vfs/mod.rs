// ABOUTME: Virtual filesystem layer dispatching archive formats to their readers.
// ABOUTME: Exposes the reader factory used by both the UI and the decode workers.
use std::cell::RefCell;
use std::path::{Path, PathBuf};

pub mod archive;
pub mod djvu;
pub mod ordering;
pub mod pdf;
pub mod rar;
pub mod zip;

pub use archive::{ArchiveError, ArchiveReader, build_pages};
pub use djvu::{DJVU_EXTENSIONS, DjvuArchiveReader};
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
        _ if DJVU_EXTENSIONS.contains(&extension.as_str()) => {
            Ok(Box::new(DjvuArchiveReader::new(path)))
        }
        _ => Err(ArchiveError::UnsupportedFormat(path.display().to_string())),
    }
}

thread_local! {
    // Caches the reader for the archive this thread last touched. Building a
    // reader per page throws away everything the format needs to be efficient:
    // the zip central directory is reparsed for every page, and the rar cursor
    // restarts from the front of the archive. One slot is enough because a
    // worker reads pages of one comic at a time.
    static READER: RefCell<Option<CachedReader>> = const { RefCell::new(None) };
}

struct CachedReader {
    path: PathBuf,
    reader: Box<dyn ArchiveReader>,
}

/// Reads a single page's raw bytes by entry path. Decode workers call this so
/// archive decompression stays off the GUI thread.
pub fn read_page_bytes(archive_path: &Path, page_path: &str) -> Result<Vec<u8>, ArchiveError> {
    READER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot
            .as_ref()
            .is_none_or(|cached| cached.path != archive_path)
        {
            // Drop the previous reader before opening the next one.
            *slot = None;
            *slot = Some(CachedReader {
                path: archive_path.to_path_buf(),
                reader: reader_for_path(archive_path)?,
            });
        }
        let cached = slot.as_mut().expect("reader cached above");
        cached.reader.read_page(page_path)
    })
}
