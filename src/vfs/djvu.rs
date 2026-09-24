// ABOUTME: Renders DjVu pages to bounded pixels for the decode pipeline.
// ABOUTME: Caches the parsed document per thread to avoid reparsing per page.
use std::cell::RefCell;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use djvu_rs::djvu_document::DjVuDocument;
use djvu_rs::djvu_render::{RenderOptions, render_pixmap};
use image::ImageFormat;

use super::archive::{ArchiveError, ArchiveReader, PageData};
use crate::library::metadata::document_metadata;
use crate::library::models::{ArchivePage, ComicMetadata};

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
            let file = std::fs::File::open(path)?;
            super::limits::check_size(file.metadata()?.len(), super::limits::MAX_DOCUMENT_BYTES)?;
            let bytes = super::limits::read_bounded(file, super::limits::MAX_DOCUMENT_BYTES)?;
            let options = djvu_rs::resource_limits::ParseOptions {
                limits: Some(djvu_rs::resource_limits::ResourceLimits {
                    max_file_bytes: Some(super::limits::MAX_DOCUMENT_BYTES),
                    max_page_pixels: Some(super::limits::MAX_SOURCE_PIXELS),
                    max_decoded_bytes: Some(256 * 1024 * 1024),
                    max_render_pixels: Some(super::limits::MAX_RASTER_PIXELS),
                    max_pages: Some(100_000),
                    max_components: Some(200_000),
                    ..Default::default()
                }),
            };
            let document = DjVuDocument::parse_with_options(&bytes, &options)
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
        if Self::page_index(path).is_none() {
            return Ok(None);
        }
        // Encoding is reserved for explicit raw-page extraction, not viewing.
        let PageData::Pixels(image) = self.page_data(path, None)? else {
            unreachable!()
        };
        let mut cursor = Cursor::new(Vec::new());
        image
            .write_to(&mut cursor, ImageFormat::Png)
            .map_err(|err| ArchiveError::Read(err.to_string()))?;
        Ok(Some(cursor.into_inner()))
    }

    fn page_data(
        &mut self,
        path: &str,
        target: Option<[u32; 2]>,
    ) -> Result<PageData, ArchiveError> {
        let page_index =
            Self::page_index(path).ok_or_else(|| ArchiveError::NotFound(path.to_owned()))?;
        with_document(&self.path, |document| {
            let page = document
                .page(page_index)
                .map_err(|_| ArchiveError::NotFound(path.to_owned()))?;
            // RenderOptions requires explicit dimensions. Bound both the source
            // and output before the renderer allocates its pixel buffer.
            let (width, height) = page.dimensions();
            let [width, height] =
                super::limits::raster_size(u32::from(width), u32::from(height), target)?;
            let options = RenderOptions {
                width,
                height,
                ..RenderOptions::default()
            };
            let pixmap =
                render_pixmap(page, &options).map_err(|err| ArchiveError::Read(err.to_string()))?;
            image::RgbaImage::from_raw(pixmap.width, pixmap.height, pixmap.data)
                .map(image::DynamicImage::ImageRgba8)
                .ok_or_else(|| {
                    ArchiveError::Read("rendered dimensions do not match pixels".to_owned())
                })
        })
        .map(PageData::Pixels)
    }

    fn document_metadata(&mut self) -> Result<Option<ComicMetadata>, ArchiveError> {
        with_document(&self.path, |document| {
            // A book with no METa chunk simply has no metadata, which is not a
            // failure; only a malformed one is.
            let Some(metadata) = document
                .metadata()
                .map_err(|err| ArchiveError::Read(err.to_string()))?
            else {
                return Ok(None);
            };
            Ok(document_metadata(metadata.title, metadata.author))
        })
    }
}

pub(crate) fn clear_document_cache() {
    DOCUMENT.with(|cell| {
        *cell.borrow_mut() = None;
    });
}
