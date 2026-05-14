use std::path::{Path, PathBuf};

use super::archive::{ArchiveError, ArchiveReader, build_pages};
use crate::library::models::ArchivePage;

pub struct RarArchiveReader {
    path: PathBuf,
}

impl RarArchiveReader {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl ArchiveReader for RarArchiveReader {
    fn list_pages(&mut self) -> Result<Vec<ArchivePage>, ArchiveError> {
        let archive = unrar::Archive::new(&self.path)
            .open_for_listing()
            .map_err(|err| ArchiveError::BackendUnavailable(err.to_string()))?;
        let paths = archive
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.filename.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        Ok(build_pages(paths))
    }

    fn read_page(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError> {
        self.read_entry(path)?
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))
    }

    fn read_entry(&mut self, path: &str) -> Result<Option<Vec<u8>>, ArchiveError> {
        let mut archive = unrar::Archive::new(&self.path)
            .open_for_processing()
            .map_err(|err| ArchiveError::BackendUnavailable(err.to_string()))?;

        while let Some(header) = archive
            .read_header()
            .map_err(|err| ArchiveError::Read(err.to_string()))?
        {
            let entry_path = header.entry().filename.to_string_lossy().into_owned();
            if entry_path == path {
                let (bytes, _next_archive) = header
                    .read()
                    .map_err(|err| ArchiveError::Read(err.to_string()))?;
                return Ok(Some(bytes));
            }
            archive = header
                .skip()
                .map_err(|err| ArchiveError::Read(err.to_string()))?;
        }

        Ok(None)
    }
}
