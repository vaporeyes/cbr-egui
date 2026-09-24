// ABOUTME: Copies comic archives into the managed library store keyed by content hash.
// ABOUTME: Produces ImportedComic records (page count, metadata) without touching the database.
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::library::errors::LibraryError;
use crate::library::models::ComicMetadata;
use crate::library::scanner::{archive_metadata, archive_page_count, is_openable_archive_path};
use crate::vfs::ArchiveError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedComic {
    pub source_path: PathBuf,
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
    if !is_openable_archive_path(source) {
        return Err(LibraryError::Archive(ArchiveError::UnsupportedFormat(
            source.display().to_string(),
        )));
    }
    if !source.is_file() {
        return Err(LibraryError::InaccessibleRoot(source.display().to_string()));
    }
    let source_path = source.canonicalize()?;

    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| LibraryError::InaccessibleRoot(source.display().to_string()))?;
    let mut stored_name = PathBuf::from(file_name);
    match source
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("zip") => {
            stored_name.set_extension("cbz");
        }
        Some("rar") => {
            stored_name.set_extension("cbr");
        }
        _ => {}
    }

    fs::create_dir_all(store_root)?;
    let suffix = format!(
        ".{}",
        source.extension().and_then(|s| s.to_str()).unwrap_or("cbz")
    );
    let mut staging = tempfile::Builder::new()
        .prefix(".import-")
        .suffix(&suffix)
        .tempfile_in(store_root)?;
    let mut input = fs::File::open(source)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        staging.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
    }
    staging.as_file().sync_all()?;
    let content_hash = hasher.finalize().to_hex().to_string();

    // Page count must be readable for the comic to be useful, so propagate
    // failures. Metadata is optional, so a malformed ComicInfo.xml is ignored
    // rather than blocking the import.
    let page_count = archive_page_count(staging.path());
    let metadata = archive_metadata(staging.path()).ok().flatten();
    crate::vfs::clear_document_cache();
    let page_count = page_count?;
    let dest_dir = store_root.join(&content_hash);
    fs::create_dir_all(&dest_dir)?;
    // Reuse an existing verified object even when the source was renamed.
    // Legacy stores retain their filenames and therefore their database IDs.
    let existing = fs::read_dir(&dest_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && is_openable_archive_path(path)
                && hash_file(path).is_ok_and(|hash| hash == content_hash)
        });
    let already_present = existing.is_some();
    let stored_path = existing.unwrap_or_else(|| dest_dir.join(stored_name));
    if !already_present {
        staging.persist(&stored_path).map_err(|error| error.error)?;
    }

    Ok(ImportedComic {
        source_path,
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
