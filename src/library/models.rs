#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comic {
    pub id: i64,
    pub path: String,
    pub hash: String,
    pub page_count: u32,
    pub metadata_id: Option<i64>,
    pub availability: ComicAvailability,
    pub thumbnail_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComicInput {
    pub path: String,
    pub hash: String,
    pub page_count: u32,
    pub metadata_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub parent_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub comic_id: i64,
    pub current_page: u32,
    pub is_read: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bookmark {
    pub comic_id: i64,
    pub page_index: u32,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComicMetadata {
    pub id: Option<i64>,
    pub series: Option<String>,
    pub title: Option<String>,
    pub number: Option<String>,
    pub writer: Option<String>,
    pub penciller: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComicMetadataDisplay {
    pub series: Option<String>,
    pub title: Option<String>,
    pub number: Option<String>,
    pub writer: Option<String>,
    pub penciller: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryComicRow {
    pub comic: Comic,
    pub metadata: ComicMetadataDisplay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivePage {
    pub path: String,
    pub sort_index: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ComicAvailability {
    #[default]
    Available,
    Unavailable,
}

impl ComicAvailability {
    pub fn as_i64(self) -> i64 {
        match self {
            Self::Available => 1,
            Self::Unavailable => 0,
        }
    }

    pub fn from_i64(value: i64) -> Self {
        if value == 0 {
            Self::Unavailable
        } else {
            Self::Available
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LibraryScanStatus {
    #[default]
    Idle,
    Scanning,
    Synchronized,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryRoot {
    pub path: String,
    pub watch_status: LibraryScanStatus,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThumbnailStatus {
    Missing,
    Loading,
    Ready { cache_path: String },
    Failed { message: String },
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverThumbnail {
    pub comic_id: i64,
    pub source_path: String,
    pub source_fingerprint: String,
    pub cache_path: String,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub status: ThumbnailStatus,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryGridItem {
    pub comic_id: i64,
    pub title: String,
    pub subtitle: Option<String>,
    pub path: String,
    pub source_fingerprint: String,
    pub page_count: u32,
    pub thumbnail_status: ThumbnailStatus,
    pub availability: ComicAvailability,
    pub series: Option<String>,
    pub series_key: Option<String>,
    pub folder_label: Option<String>,
    pub folder_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LibraryGroupKind {
    Series,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryGroup {
    pub kind: LibraryGroupKind,
    pub key: String,
    pub label: String,
    pub item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveLibraryFilter {
    pub kind: LibraryGroupKind,
    pub key: String,
}
