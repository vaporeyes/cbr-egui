// ABOUTME: Copies comic archives into the managed library store keyed by content hash.
// ABOUTME: Produces ImportedComic records (page count, metadata) without touching the database.
use std::fs;
use std::path::{Path, PathBuf};

use crate::library::errors::LibraryError;
use crate::library::models::ComicMetadata;
use crate::library::scanner::{archive_metadata, archive_page_count, is_supported_archive_path};
use crate::vfs::ArchiveError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedComic {
    pub stored_path: PathBuf,
    pub content_hash: String,
    pub page_count: u32,
    pub metadata: Option<ComicMetadata>,
    pub already_present: bool,
}

#[derive(Debug, Default)]
pub struct ImportSummary {
    pub imported: Vec<ImportedComic>,
    pub failures: Vec<(PathBuf, String)>,
}

/// Copies a single archive into the managed store and reads its page count and
/// metadata from the stored copy. The destination is namespaced by the file's
/// content hash so re-importing the same bytes is idempotent.
pub fn import_comic_file(source: &Path, store_root: &Path) -> Result<ImportedComic, LibraryError> {
    if !is_supported_archive_path(source) {
        return Err(LibraryError::Archive(ArchiveError::UnsupportedFormat(
            source.display().to_string(),
        )));
    }
    if !source.is_file() {
        return Err(LibraryError::InaccessibleRoot(source.display().to_string()));
    }

    let content_hash = hash_file(source)?;
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| LibraryError::InaccessibleRoot(source.display().to_string()))?;

    let dest_dir = store_root.join(&content_hash);
    let stored_path = dest_dir.join(file_name);
    let already_present = stored_path.exists();
    if !already_present {
        fs::create_dir_all(&dest_dir)?;
        fs::copy(source, &stored_path)?;
    }

    // Page count must be readable for the comic to be useful, so propagate
    // failures. Metadata is optional, so a malformed ComicInfo.xml is ignored
    // rather than blocking the import.
    let page_count = archive_page_count(&stored_path)?;
    let metadata = archive_metadata(&stored_path).ok().flatten();

    Ok(ImportedComic {
        stored_path,
        content_hash,
        page_count,
        metadata,
        already_present,
    })
}

/// Imports a batch of source paths, collecting per-file failures so a single
/// unreadable archive does not abort the rest of a library import.
pub fn import_paths(sources: &[PathBuf], store_root: &Path) -> ImportSummary {
    let mut summary = ImportSummary::default();
    for source in sources {
        match import_comic_file(source, store_root) {
            Ok(imported) => summary.imported.push(imported),
            Err(error) => summary.failures.push((source.clone(), error.to_string())),
        }
    }
    summary
}

fn hash_file(path: &Path) -> Result<String, LibraryError> {
    let mut hasher = blake3::Hasher::new();
    let mut file = fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}
