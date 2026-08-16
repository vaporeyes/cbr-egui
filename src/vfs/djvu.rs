// ABOUTME: Renders DjVu pages to PNG bytes for the unified decode pipeline.
// ABOUTME: Caches the parsed document per thread to avoid reparsing per page.
use std::cell::RefCell;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use djvu_rs::djvu_document::DjVuDocument;
use djvu_rs::djvu_render::{RenderOptions, render_pixmap};
use image::ImageFormat;

use super::archive::{ArchiveError, ArchiveReader};
use crate::library::models::ArchivePage;

/// Extensions handled by this reader. DjVu is published under both the long
/// name and the historical 8.3 short form.
pub const DJVU_EXTENSIONS: &[&str] = &["djvu", "djv"];

thread_local! {
    // Caches the most recently opened document per thread, keyed by path, so
    // sequential page reads of the same book skip reparsing it from disk.
    // A DjVu document owns its decoded structure, so unlike the PDF reader
    // there is no shared binding to keep alive alongside it.
    static DOCUMENT: RefCell<Option<CachedDocument>> = const { RefCell::new(None) };
}

struct CachedDocument {
    path: PathBuf,
    document: DjVuDocument,
}

pub struct DjvuArchiveReader {
    path: PathBuf,
}

impl DjvuArchiveReader {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Parses a synthetic `page_<n>.png` entry name into a zero-based index.
    /// Returns None for any other name, which is an absence rather than a
    /// failure: callers probe for entries like ComicInfo.xml that a DjVu book
    /// simply does not have.
    fn page_index(path: &str) -> Option<usize> {
        path.strip_prefix("page_")
            .and_then(|value| value.strip_suffix(".png"))
            .and_then(|value| value.parse::<usize>().ok())
            .and_then(|number| number.checked_sub(1))
    }
}

/// Runs a closure against the parsed document for `path`, opening and caching
/// it per thread when the cached document is for a different file.
fn with_document<R>(
    path: &Path,
    f: impl FnOnce(&DjVuDocument) -> Result<R, ArchiveError>,
) -> Result<R, ArchiveError> {
    DOCUMENT.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.as_ref().is_none_or(|cached| cached.path != path) {
            // Drop the previously cached document before parsing the new one,
            // so two books are never held in memory at once.
            *slot = None;
            let bytes = std::fs::read(path)?;
            let document = DjVuDocument::parse(&bytes)
                .map_err(|err| ArchiveError::CorruptArchive(err.to_string()))?;
            *slot = Some(CachedDocument {
                path: path.to_path_buf(),
                document,
            });
        }
        let cached = slot.as_ref().expect("document cached above");
        f(&cached.document)
    })
}

impl ArchiveReader for DjvuArchiveReader {
    fn list_pages(&mut self) -> Result<Vec<ArchivePage>, ArchiveError> {
        with_document(&self.path, |document| {
            Ok((0..document.page_count())
                .map(|index| ArchivePage {
                    path: format!("page_{}.png", index + 1),
                    sort_index: index,
                })
                .collect())
        })
    }

    fn read_page(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError> {
        self.read_entry(path)?
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))
    }

    fn read_entry(&mut self, path: &str) -> Result<Option<Vec<u8>>, ArchiveError> {
        let Some(page_index) = Self::page_index(path) else {
            return Ok(None);
        };
        with_document(&self.path, |document| {
            let page = document
                .page(page_index)
                .map_err(|_| ArchiveError::NotFound(path.to_owned()))?;

            // Render at the page's own pixel dimensions. RenderOptions leaves
            // width and height at zero by default, which the renderer rejects
            // rather than treating as "native size".
            let (width, height) = page.dimensions();
            let options = RenderOptions {
                width: u32::from(width),
                height: u32::from(height),
                ..RenderOptions::default()
            };
            let pixmap =
                render_pixmap(page, &options).map_err(|err| ArchiveError::Read(err.to_string()))?;

            // Hand the decode pipeline encoded bytes like every other reader,
            // so page decoding, limits, rotation, and adjustments stay in one
            // place. Matches how the PDF reader bridges a rendered page.
            let image = image::RgbaImage::from_raw(pixmap.width, pixmap.height, pixmap.data)
                .ok_or_else(|| {
                    ArchiveError::Read(
                        "rendered page dimensions do not match its pixels".to_owned(),
                    )
                })?;
            let mut cursor = Cursor::new(Vec::new());
            image
                .write_to(&mut cursor, ImageFormat::Png)
                .map_err(|err| ArchiveError::Read(err.to_string()))?;
            Ok(Some(cursor.into_inner()))
        })
    }
}
