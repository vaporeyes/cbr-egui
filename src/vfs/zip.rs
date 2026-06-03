use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use super::archive::{ArchiveError, ArchiveReader, build_pages};
use crate::library::models::ArchivePage;

pub struct ZipArchiveReader {
    path: PathBuf,
    pages_cache: Option<Vec<ArchivePage>>,
}

impl ZipArchiveReader {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            pages_cache: None,
        }
    }

    fn open(&self) -> Result<ZipArchive<File>, ArchiveError> {
        let file = File::open(&self.path)?;
        ZipArchive::new(file).map_err(|err| ArchiveError::CorruptArchive(err.to_string()))
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
        let mut archive = self.open()?;
        let Ok(mut file) = archive.by_name(path) else {
            return Ok(None);
        };
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes)
            .map_err(|err| ArchiveError::Read(err.to_string()))?;
        Ok(Some(bytes))
    }
}
