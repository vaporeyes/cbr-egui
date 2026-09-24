// ABOUTME: Reads ZIP comic entries using a retained central directory and handle.
// ABOUTME: Bounds decompression before allocating page or metadata buffers.
use std::fs::File;
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use super::archive::{ArchiveError, ArchiveReader, build_pages};
use crate::library::models::ArchivePage;

pub struct ZipArchiveReader {
    path: PathBuf,
    pages_cache: Option<Vec<ArchivePage>>,
    archive: Option<ZipArchive<File>>,
}

impl ZipArchiveReader {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            pages_cache: None,
            archive: None,
        }
    }

    fn open(&mut self) -> Result<&mut ZipArchive<File>, ArchiveError> {
        if self.archive.is_none() {
            let file = File::open(&self.path)?;
            self.archive = Some(
                ZipArchive::new(file)
                    .map_err(|err| ArchiveError::CorruptArchive(err.to_string()))?,
            );
        }
        Ok(self.archive.as_mut().expect("archive opened above"))
    }
}

impl ArchiveReader for ZipArchiveReader {
    fn list_pages(&mut self) -> Result<Vec<ArchivePage>, ArchiveError> {
        if let Some(pages) = &self.pages_cache {
            return Ok(pages.clone());
        }
        let archive = self.open()?;
        let paths = archive
            .file_names()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let pages = build_pages(paths);
        self.pages_cache = Some(pages.clone());
        Ok(pages)
    }

    fn read_page(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError> {
        self.read_entry(path)?
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))
    }

    fn read_entry(&mut self, path: &str) -> Result<Option<Vec<u8>>, ArchiveError> {
        // No raw-bytes cache: decoded pages are already cached as textures, so the
        // OS page cache is left to serve repeat reads. This keeps the VFS layer's
        // baseline memory footprint minimal.
        let archive = self.open()?;
        let file = match archive.by_name(path) {
            Ok(file) => file,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(error) => return Err(ArchiveError::Read(error.to_string())),
        };
        let limit = super::limits::entry_limit(path);
        super::limits::check_size(file.size(), limit)?;
        super::limits::read_bounded(file, limit).map(Some)
    }
}
