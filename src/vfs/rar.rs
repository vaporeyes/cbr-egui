use std::path::{Path, PathBuf};

use unrar::{CursorBeforeHeader, OpenArchive, Process};

use super::archive::{ArchiveError, ArchiveReader, build_pages};
use crate::library::models::ArchivePage;

pub struct RarArchiveReader {
    path: PathBuf,
    /// Processing handle parked at the next header. RAR is a sequential
    /// format with no index, so reaching an entry means walking to it. Holding
    /// the cursor lets the next read continue from where the last one stopped
    /// rather than walking from the start again, which is what turns a
    /// front-to-back read of the archive from quadratic into linear.
    cursor: Option<OpenArchive<Process, CursorBeforeHeader>>,
}

impl RarArchiveReader {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            cursor: None,
        }
    }

    /// Walks forward from the parked cursor (or from the start when there is
    /// none) looking for `path`. Returns None on reaching the end of the
    /// archive without a match, leaving no cursor behind.
    fn scan_forward(&mut self, path: &str) -> Result<Option<Vec<u8>>, ArchiveError> {
        let mut archive = match self.cursor.take() {
            Some(archive) => archive,
            None => unrar::Archive::new(&self.path)
                .open_for_processing()
                .map_err(|err| ArchiveError::BackendUnavailable(err.to_string()))?,
        };

        while let Some(header) = archive
            .read_header()
            .map_err(|err| ArchiveError::Read(err.to_string()))?
        {
            let entry_path = header.entry().filename.to_string_lossy().into_owned();
            if entry_path == path {
                let (bytes, next) = header
                    .read()
                    .map_err(|err| ArchiveError::Read(err.to_string()))?;
                self.cursor = Some(next);
                return Ok(Some(bytes));
            }
            archive = header
                .skip()
                .map_err(|err| ArchiveError::Read(err.to_string()))?;
        }

        Ok(None)
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
        let resumed = self.cursor.is_some();
        if let Some(bytes) = self.scan_forward(path)? {
            return Ok(Some(bytes));
        }
        if !resumed {
            // That pass already covered the whole archive.
            return Ok(None);
        }
        // The entry sits behind where we resumed from. scan_forward leaves no
        // cursor when it runs out, so this restarts from the beginning.
        self.scan_forward(path)
    }
}
