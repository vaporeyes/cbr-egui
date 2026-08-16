use std::path::Path;

use thiserror::Error;

use crate::library::models::{ArchivePage, ComicMetadata};

use super::ordering::{is_page_image_path, sort_natural};

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("unsupported archive format: {0}")]
    UnsupportedFormat(String),
    #[error("archive backend is unavailable: {0}")]
    BackendUnavailable(String),
    #[error("corrupt archive: {0}")]
    CorruptArchive(String),
    #[error("page not found: {0}")]
    NotFound(String),
    #[error("archive read failed: {0}")]
    Read(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub trait ArchiveReader {
    fn list_pages(&mut self) -> Result<Vec<ArchivePage>, ArchiveError>;
    fn read_page(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError>;
    fn read_entry(&mut self, path: &str) -> Result<Option<Vec<u8>>, ArchiveError>;

    /// Metadata carried by the document format itself.
    ///
    /// Defaults to none, which is correct for the archive formats: a zip or rar
    /// has no document-level metadata of its own, only whatever ComicInfo.xml
    /// entry it contains, and `read_entry` already exposes that. Document
    /// formats such as PDF and DjVu override this.
    fn document_metadata(&mut self) -> Result<Option<ComicMetadata>, ArchiveError> {
        Ok(None)
    }
}

pub fn build_pages<I, S>(paths: I) -> Vec<ArchivePage>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut page_paths = paths
        .into_iter()
        .map(Into::into)
        .filter(|path| is_page_image_path(path))
        .collect::<Vec<_>>();
    sort_natural(&mut page_paths);
    page_paths
        .into_iter()
        .enumerate()
        .map(|(sort_index, path)| ArchivePage { path, sort_index })
        .collect()
}

pub fn unsupported_rar_error(path: &Path) -> ArchiveError {
    ArchiveError::BackendUnavailable(format!(
        "RAR support requires a native backend; cannot read {}",
        path.display()
    ))
}
