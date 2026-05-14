# Data Model: Domain Models & Data Layer

## Comic

Represents one indexed comic archive.

**Fields**

- `id: i64`: Stable storage identifier.
- `path: String`: Comic path exactly as provided by importer/scanner.
- `hash: String`: Caller-supplied content identity.
- `page_count: u32`: Number of listed readable pages.
- `metadata_id: Option<i64>`: Optional link to parsed metadata.

**Relationships**

- May belong to one Folder through future scan/import association.
- May have one Progress row.
- May reference one ComicMetadata row.

**Validation**

- `path` is required and unique for duplicate-path handling.
- `hash` is required.
- `page_count` must fit the known archive page count.
- `metadata_id` may be null.

## Folder

Represents a root import folder or nested library folder.

**Fields**

- `id: i64`: Stable storage identifier.
- `path: String`: Folder path exactly as provided by importer/scanner.
- `parent_id: Option<i64>`: Parent folder, or null for root folders.

**Relationships**

- May have many child folders.
- May be referenced by imported comics in later scan features.

**Validation**

- `path` is required.
- `parent_id` must reference an existing folder when present.
- Root folders use null `parent_id`.

## Progress

Represents current reading state for one comic.

**Fields**

- `comic_id: i64`: Comic identifier and unique progress key.
- `current_page: u32`: Last/current page index.
- `is_read: bool`: Whether the comic is marked complete.

**Relationships**

- Belongs to exactly one Comic.

**Validation**

- `comic_id` must reference an existing comic.
- Only one Progress row may exist per comic.
- If `current_page` exceeds known page count after archive changes, storage may
  preserve it but service callers should clamp or reconcile during reader/import
  workflows.

**State Transitions**

- Missing -> created when progress is first saved.
- Existing -> replaced by update/upsert for the same comic.
- Any -> `is_read = true` when caller marks complete.

## ComicMetadata

Represents parsed embedded ComicInfo.xml fields for this feature.

**Fields**

- `id: i64`: Stable storage identifier.
- `title: Option<String>`
- `number: Option<String>`
- `writer: Option<String>`
- `penciller: Option<String>`

**Relationships**

- May be referenced by many Comics if later importer chooses deduplication, but
  this feature expects a comic to link to at most one metadata row.

**Validation**

- All parsed fields are optional.
- Missing ComicInfo.xml produces empty metadata rather than a failure.
- Malformed XML returns a recoverable metadata error and does not block archive
  page listing.

## ArchivePage

Represents one readable page entry inside an archive.

**Fields**

- `path: String`: Archive-relative path.
- `sort_index: usize`: Natural order position after filtering.

**Relationships**

- Belongs to an archive reader instance.

**Validation**

- Must be an image-like entry (`jpg`, `jpeg`, `png`, `gif`, `webp`, `bmp`, or
  other approved image extension).
- Hidden metadata directories such as `__MACOSX`, dot-directories, and non-page
  files are excluded.
- Natural ordering compares numeric path portions by numeric value.
