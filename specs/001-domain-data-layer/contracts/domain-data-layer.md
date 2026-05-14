# Contracts: Domain Data Layer

## LibraryService Contract

LibraryService owns database access and exposes storage operations to import,
metadata, and future UI-facing workflows. ViewerState must not call `rusqlite`
directly.

```rust
pub struct LibraryService { /* storage-owned internals */ }

impl LibraryService {
    pub fn initialize(db_path: &std::path::Path) -> Result<Self, LibraryError>;
    pub fn create_folder(&self, path: &str, parent_id: Option<i64>) -> Result<Folder, LibraryError>;
    pub fn upsert_comic(&self, input: ComicInput) -> Result<Comic, LibraryError>;
    pub fn get_comic(&self, id: i64) -> Result<Option<Comic>, LibraryError>;
    pub fn save_progress(&self, comic_id: i64, current_page: u32, is_read: bool) -> Result<Progress, LibraryError>;
    pub fn get_progress(&self, comic_id: i64) -> Result<Option<Progress>, LibraryError>;
}
```

**Required behavior**

- Schema initialization creates comics, folders, progress, and metadata storage
  in one setup flow.
- Comic paths are unique; duplicate path imports update or return the existing
  record according to implementation tasks.
- Progress save is an upsert keyed by `comic_id`.
- Storage errors are returned as `LibraryError` and never panic.

## Metadata Parser Contract

```rust
pub fn parse_comic_info_xml(bytes: &[u8]) -> Result<ComicMetadata, MetadataError>;
pub fn read_archive_metadata(reader: &dyn ArchiveReader) -> Result<Option<ComicMetadata>, MetadataError>;
```

**Required behavior**

- Extract `Title`, `Number`, `Writer`, and `Penciller` when present.
- Missing `ComicInfo.xml` returns `Ok(None)`.
- Missing individual fields return `None` fields.
- Malformed XML returns `Err(MetadataError::Malformed(_))` and leaves archive
  page listing usable.

## ArchiveReader Contract

```rust
pub trait ArchiveReader {
    fn list_pages(&mut self) -> Result<Vec<ArchivePage>, ArchiveError>;
    fn read_page(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError>;
    fn read_entry(&mut self, path: &str) -> Result<Option<Vec<u8>>, ArchiveError>;
}
```

**Required behavior**

- ZIP and RAR readers implement this trait.
- The RAR reader uses `7z` when available and returns
  `ArchiveError::BackendUnavailable` if the backend executable is missing.
- `list_pages` returns image-like entries only, sorted in natural reading order.
- Hidden metadata directories and non-page files are excluded.
- `read_page` reads only the requested page payload and returns
  `ArchiveError::NotFound` or a backend error for missing/corrupt entries.
- Normal full-archive extraction to disk is forbidden.

## Error Contract

Errors are typed and recoverable:

- `LibraryError`: schema, constraint, transaction, and SQLite failures.
- `MetadataError`: absent metadata, malformed XML, unsupported encoding, and
  archive-entry read failures.
- `ArchiveError`: unsupported format, corrupt archive, missing page, read
  failure, and backend unavailability.

Callers must be able to continue listing pages when metadata parsing fails.
