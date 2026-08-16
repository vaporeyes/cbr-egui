// ABOUTME: Renders PDF pages to PNG bytes for the unified decode pipeline.
// ABOUTME: Caches the PDFium binding and parsed document per thread to avoid rebinding/reparsing per page.
use std::cell::RefCell;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::ImageFormat;
use pdfium_render::prelude::{PdfDocument, PdfRenderConfig, Pdfium};

use super::archive::{ArchiveError, ArchiveReader};
use crate::library::models::ArchivePage;

thread_local! {
    // The PDFium binding is leaked once per thread so parsed documents can borrow
    // it for 'static. The binding lives for the whole process regardless, so the
    // leak is bounded to one per worker thread.
    static PDFIUM: RefCell<Option<&'static Pdfium>> = const { RefCell::new(None) };
    // Caches the most recently opened document per thread, keyed by path, so
    // sequential page reads of the same comic skip re-parsing the PDF from disk.
    static DOCUMENT: RefCell<Option<CachedDocument>> = const { RefCell::new(None) };
}

struct CachedDocument {
    path: PathBuf,
    document: PdfDocument<'static>,
}

pub struct PdfArchiveReader {
    path: PathBuf,
}

impl PdfArchiveReader {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Parses a synthetic `page_<n>.png` entry name into a zero-based index.
    /// Returns None for any other name, which is an absence rather than a
    /// failure: callers probe for entries like ComicInfo.xml that a PDF simply
    /// does not have.
    fn page_index(path: &str) -> Option<i32> {
        path.strip_prefix("page_")
            .and_then(|value| value.strip_suffix(".png"))
            .and_then(|value| value.parse::<i32>().ok())
            .and_then(|index| index.checked_sub(1))
            .filter(|index| *index >= 0)
    }
}

/// Returns the thread-local PDFium binding, binding to the system library on
/// first use. The binding is leaked to obtain a 'static reference that cached
/// documents can borrow.
fn thread_pdfium() -> Result<&'static Pdfium, ArchiveError> {
    PDFIUM.with(|cell| {
        if let Some(pdfium) = *cell.borrow() {
            return Ok(pdfium);
        }
        let bindings = Pdfium::bind_to_system_library()
            .map_err(|err| ArchiveError::BackendUnavailable(err.to_string()))?;
        let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
        *cell.borrow_mut() = Some(pdfium);
        Ok(pdfium)
    })
}

/// Runs a closure against the parsed document for `path`, opening and caching it
/// per thread when the cached document is for a different file.
fn with_document<R>(
    path: &Path,
    f: impl FnOnce(&PdfDocument<'static>) -> Result<R, ArchiveError>,
) -> Result<R, ArchiveError> {
    let pdfium = thread_pdfium()?;
    DOCUMENT.with(|cell| {
        let mut slot = cell.borrow_mut();
        let matches = slot.as_ref().is_some_and(|cached| cached.path == path);
        if !matches {
            // Drop the previously cached document before opening the new one.
            *slot = None;
            let document = pdfium
                .load_pdf_from_file(path, None)
                .map_err(|err| ArchiveError::Read(err.to_string()))?;
            *slot = Some(CachedDocument {
                path: path.to_path_buf(),
                document,
            });
        }
        let cached = slot.as_ref().expect("document cached above");
        f(&cached.document)
    })
}

impl ArchiveReader for PdfArchiveReader {
    fn list_pages(&mut self) -> Result<Vec<ArchivePage>, ArchiveError> {
        with_document(&self.path, |document| {
            Ok((0..document.pages().len())
                .map(|index| ArchivePage {
                    path: format!("page_{}.png", index + 1),
                    sort_index: index as usize,
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
                .pages()
                .get(page_index)
                .map_err(|_| ArchiveError::NotFound(path.to_owned()))?;
            let image = page
                .render_with_config(&PdfRenderConfig::new())
                .map_err(|err| ArchiveError::Read(err.to_string()))?
                .as_image()
                .map_err(|err| ArchiveError::Read(err.to_string()))?;
            let mut cursor = Cursor::new(Vec::new());
            image
                .write_to(&mut cursor, ImageFormat::Png)
                .map_err(|err| ArchiveError::Read(err.to_string()))?;
            Ok(Some(cursor.into_inner()))
        })
    }
}
