use thiserror::Error;

use crate::vfs::ArchiveError;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("archive error: {0}")]
    Archive(#[from] ArchiveError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("watch error: {0}")]
    Watch(String),
    #[error("thumbnail error: {0}")]
    Thumbnail(String),
    #[error("inaccessible library root: {0}")]
    InaccessibleRoot(String),
    #[error("invalid parent folder id: {0}")]
    InvalidParent(i64),
    #[error("integer conversion failed for {field}: {value}")]
    IntegerConversion { field: &'static str, value: i64 },
}

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("malformed ComicInfo.xml: {0}")]
    Malformed(String),
    #[error("archive entry read failed: {0}")]
    Archive(#[from] ArchiveError),
}
