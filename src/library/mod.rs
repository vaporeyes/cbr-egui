pub mod errors;
pub mod import;
pub mod metadata;
pub mod models;
pub mod scanner;
pub mod service;
pub mod storage;
pub mod thumbnails;

pub use errors::{LibraryError, MetadataError};
pub use import::{ImportSummary, ImportedComic, import_comic_file, import_paths};
pub use metadata::{parse_comic_info_xml, read_archive_metadata};
pub use models::{
    ActiveLibraryFilter, ArchivePage, Bookmark, Comic, ComicAvailability, ComicInput,
    ComicMetadata, ComicMetadataDisplay, CoverThumbnail, Folder, LibraryComicRow, LibraryGridItem,
    LibraryGroup, LibraryGroupKind, LibraryRoot, LibraryScanStatus, Progress, ThumbnailStatus,
};
pub use scanner::{
    ScannedComic, archive_page_count, discover_supported_archives, is_supported_archive_path,
    scan_library_root, source_fingerprint,
};
pub use service::LibraryService;
pub use thumbnails::{
    MAX_THUMBNAIL_HEIGHT, ThumbnailCacheEntry, ThumbnailCacheError, ThumbnailRequest,
    ThumbnailResult, ThumbnailWorkerPool, cache_key_for_source, cache_path_for_source,
    cover_request_for_pages, is_thumbnail_stale, thumbnail_target_size,
};
