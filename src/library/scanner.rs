use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::library::errors::LibraryError;
use crate::library::metadata::read_archive_metadata;
use crate::library::models::ComicMetadata;
use crate::vfs::ArchiveReader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedComic {
    pub path: String,
    pub fingerprint: String,
    pub page_count: u32,
    pub metadata: Option<ComicMetadata>,
}

pub fn is_supported_archive_path(path: impl AsRef<Path>) -> bool {
    let Some(extension) = path
        .as_ref()
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
    else {
        return false;
    };

    matches!(extension.as_str(), "cbz" | "cbr" | "pdf")
}

pub fn discover_supported_archives(root: impl AsRef<Path>) -> Result<Vec<PathBuf>, LibraryError> {
    let root = root.as_ref();
    if !root.is_dir() {
        return Err(LibraryError::InaccessibleRoot(root.display().to_string()));
    }

    let mut archives = Vec::new();
    discover_into(root, &mut archives, 0)?;
    archives.sort();
    Ok(archives)
}

/// Deepest directory nesting walked during discovery. Comic libraries are a
/// few levels deep at most, so this only ever trips on pathological trees.
const MAX_DISCOVERY_DEPTH: usize = 32;

fn discover_into(
    dir: &Path,
    archives: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), LibraryError> {
    if depth >= MAX_DISCOVERY_DEPTH {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if file_name.starts_with('.') || file_name == "__MACOSX" {
            continue;
        }
        // file_type does not follow symlinks, unlike Path::is_dir. A link
        // pointing at one of its own ancestors would otherwise recurse until
        // the stack is exhausted, which aborts the process rather than
        // surfacing an error.
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            discover_into(&path, archives, depth + 1)?;
        } else if is_supported_archive_path(&path) {
            archives.push(path);
        }
    }
    Ok(())
}

pub fn source_fingerprint(path: impl AsRef<Path>) -> Result<String, LibraryError> {
    let metadata = fs::metadata(path.as_ref())?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    Ok(format!("{}:{modified}", metadata.len()))
}

pub fn archive_page_count(path: impl AsRef<Path>) -> Result<u32, LibraryError> {
    let mut reader = archive_reader_for_path(path.as_ref())?;
    Ok(reader.list_pages()?.len() as u32)
}

pub fn archive_metadata(path: impl AsRef<Path>) -> Result<Option<ComicMetadata>, LibraryError> {
    let mut reader = archive_reader_for_path(path.as_ref())?;
    read_archive_metadata(&mut *reader).map_err(LibraryError::from)
}

fn archive_reader_for_path(path: &Path) -> Result<Box<dyn ArchiveReader>, LibraryError> {
    crate::vfs::reader_for_path(path).map_err(LibraryError::from)
}

fn archive_scan_details(path: &Path) -> (u32, Option<ComicMetadata>) {
    let Ok(mut reader) = archive_reader_for_path(path) else {
        return (0, None);
    };
    let page_count = reader
        .list_pages()
        .map(|pages| pages.len() as u32)
        .unwrap_or(0);
    let metadata = read_archive_metadata(&mut *reader).ok().flatten();
    (page_count, metadata)
}

pub fn scan_library_root(root: impl AsRef<Path>) -> Result<Vec<ScannedComic>, LibraryError> {
    discover_supported_archives(root)?
        .into_iter()
        .map(|path| {
            let fingerprint = source_fingerprint(&path)?;
            let (page_count, metadata) = archive_scan_details(&path);
            Ok(ScannedComic {
                path: path.to_string_lossy().into_owned(),
                fingerprint,
                page_count,
                metadata,
            })
        })
        .collect()
}
