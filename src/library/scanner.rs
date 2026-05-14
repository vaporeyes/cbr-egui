use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::library::errors::LibraryError;
use crate::vfs::{ArchiveReader, PdfArchiveReader, RarArchiveReader, ZipArchiveReader};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedComic {
    pub path: String,
    pub fingerprint: String,
    pub page_count: u32,
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
    discover_into(root, &mut archives)?;
    archives.sort();
    Ok(archives)
}

fn discover_into(dir: &Path, archives: &mut Vec<PathBuf>) -> Result<(), LibraryError> {
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
        if path.is_dir() {
            discover_into(&path, archives)?;
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
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    let pages = match extension.as_str() {
        "cbz" | "zip" => ZipArchiveReader::new(path).list_pages()?,
        "cbr" | "rar" => RarArchiveReader::new(path).list_pages()?,
        "pdf" => PdfArchiveReader::new(path).list_pages()?,
        _ => Vec::new(),
    };

    Ok(pages.len() as u32)
}

pub fn scan_library_root(root: impl AsRef<Path>) -> Result<Vec<ScannedComic>, LibraryError> {
    discover_supported_archives(root)?
        .into_iter()
        .map(|path| {
            let fingerprint = source_fingerprint(&path)?;
            let page_count = archive_page_count(&path).unwrap_or(0);
            Ok(ScannedComic {
                path: path.to_string_lossy().into_owned(),
                fingerprint,
                page_count,
            })
        })
        .collect()
}
