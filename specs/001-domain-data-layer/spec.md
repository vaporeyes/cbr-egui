# Feature Specification: Domain Models & Data Layer

**Feature Branch**: `001-domain-data-layer`  
**Created**: 2026-05-13  
**Status**: Draft  
**Input**: User description: "Domain Models & Data Layer. Deconstruct the data
structures before touching the UI. Initialize persistent library records for
comics, folders, and progress; parse ComicInfo.xml metadata; build archive VFS
readers for ZIP and RAR with natural page sorting."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Index Comic Library Records (Priority: P1)

As a reader building a library, I need comic files, folders, and reading
progress represented consistently so the app can track my collection before any
viewer UI exists.

**Why this priority**: Library persistence is the foundation for scanning,
metadata display, progress tracking, and later viewer workflows.

**Independent Test**: Can be tested by creating a small library with nested
folders and two comics, saving progress for one comic, and confirming the stored
records can be read back with correct relationships.

**Acceptance Scenarios**:

1. **Given** a new library database, **When** the library schema is initialized,
   **Then** storage exists for comics, folders, and per-comic reading progress.
2. **Given** a comic path, content identity, page count, and metadata reference,
   **When** the comic is stored, **Then** it can be retrieved with the same path,
   identity, page count, and metadata link.
3. **Given** a comic with saved reading progress, **When** progress is updated,
   **Then** the current page and read state replace the previous values for that
   comic.

---

### User Story 2 - Read Embedded Comic Metadata (Priority: P2)

As a reader importing comics, I want embedded comic metadata recognized so title,
issue number, and creator credits can be shown accurately during library
management.

**Why this priority**: Metadata makes imported collections usable and provides
the data later UI screens need for sorting and display.

**Independent Test**: Can be tested with an archive containing `ComicInfo.xml`
and verifying that standard fields are extracted while missing optional fields
do not prevent import.

**Acceptance Scenarios**:

1. **Given** an archive containing `ComicInfo.xml` with Title, Number, Writer,
   and Penciller values, **When** metadata is parsed, **Then** those fields are
   available to the library import workflow.
2. **Given** an archive without `ComicInfo.xml`, **When** metadata is requested,
   **Then** import continues with empty metadata rather than failing.
3. **Given** a malformed metadata file, **When** metadata is parsed, **Then** the
   caller receives a recoverable metadata error and the archive remains usable.

---

### User Story 3 - List and Read Archive Pages (Priority: P3)

As a reader opening a comic archive, I need a common archive interface that lists
pages in human reading order and returns page bytes on demand so later decoding
and viewing can be built without archive-specific UI code.

**Why this priority**: The archive VFS is required before page decode, cache,
prefetch, and viewer work can depend on stable page access semantics.

**Independent Test**: Can be tested with ZIP and RAR archives containing pages
named `page_1.jpg`, `page_2.jpg`, and `page_10.jpg`, confirming list order and
on-demand byte retrieval.

**Acceptance Scenarios**:

1. **Given** an archive with numbered page image names, **When** pages are
   listed, **Then** paths appear in natural reading order with `page_2` before
   `page_10`.
2. **Given** a listed page path, **When** page bytes are requested, **Then** the
   exact page payload is returned without extracting the full archive.
3. **Given** hidden metadata directories or non-page entries, **When** pages are
   listed, **Then** those entries are excluded from the page list.

### Edge Cases

- A library folder has no parent because it is a root import folder.
- The same comic path is encountered more than once during import.
- Reading progress is saved for a page beyond the known page count after an
  archive changes.
- `ComicInfo.xml` is missing, partially populated, uses unexpected casing, or is
  malformed.
- Archives contain hidden directories, nested folders, non-image files, or page
  names that require natural sorting.
- A requested page path no longer exists, is corrupt, or cannot be read.
- Large archives are listed without loading all page payloads into memory.
- Archive listing and metadata extraction remain suitable for background
  execution so future reader workflows can stay responsive.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST initialize library storage for Comics with `id`,
  `path`, `hash`, `page_count`, and `metadata_id` fields.
- **FR-002**: System MUST initialize library storage for Folders with `id`,
  `path`, and `parent_id` fields, including root folders without a parent.
- **FR-003**: System MUST initialize library storage for Progress with
  `comic_id`, `current_page`, and `is_read` fields.
- **FR-004**: System MUST support creating, reading, and updating progress for a
  comic without creating duplicate progress records for the same comic.
- **FR-005**: System MUST extract Title, Number, Writer, and Penciller metadata
  from embedded `ComicInfo.xml` when present in a comic archive.
- **FR-006**: System MUST treat absent or incomplete comic metadata as
  recoverable so the comic can still be indexed.
- **FR-007**: System MUST report malformed embedded metadata as a recoverable
  error that does not prevent page listing.
- **FR-008**: System MUST expose a common archive reader capability that lists
  page paths and retrieves bytes for a requested page path.
- **FR-009**: System MUST support page listing and page byte retrieval for ZIP
  comic archives.
- **FR-010**: System MUST support page listing and page byte retrieval for RAR
  comic archives.
- **FR-011**: System MUST sort archive page paths naturally so numeric portions
  sort by value rather than lexicographic text order.
- **FR-012**: System MUST exclude hidden metadata directories and non-page
  entries from the page list.
- **FR-013**: System MUST retrieve page bytes on demand without extracting the
  entire archive payload to disk.
- **FR-014**: System MUST preserve the LibraryService and ViewerState boundary
  by keeping these data-layer capabilities independent of UI state.
- **FR-015**: System MUST keep archive enumeration, metadata extraction, and
  page-byte retrieval suitable for background execution so future reader work
  can avoid blocking user interaction.

### Key Entities *(include if feature involves data)*

- **Comic**: A library entry for a comic archive. Key attributes include a stable
  identifier, absolute or library-relative path, content hash, page count, and
  optional metadata reference.
- **Folder**: A library folder node used to represent import roots and nested
  organization. Key attributes include identifier, path, and optional parent
  folder.
- **Progress**: Per-comic reading state. Key attributes include comic reference,
  current page, and read completion flag.
- **Comic Metadata**: Parsed descriptive information from embedded comic
  metadata. Key attributes for this feature are title, issue number, writer, and
  penciller.
- **Archive Page**: A readable page entry inside an archive. Key attributes are
  archive-relative path, natural sort position, and retrievable byte payload.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A new library store can be initialized and queried for required
  comic, folder, and progress structures in one setup flow.
- **SC-002**: Import logic can persist and read back at least 1,000 comic records
  with folder relationships and progress records without data loss.
- **SC-003**: Metadata extraction returns the four target fields from a valid
  embedded metadata file in at least 95% of test archives that include those
  fields.
- **SC-004**: A missing or malformed metadata file does not block archive page
  listing in 100% of tested cases.
- **SC-005**: Page listing orders numeric page names correctly for representative
  one-, two-, and three-digit page sequences.
- **SC-006**: Page byte retrieval reads a requested page without materializing
  all pages from the archive into application memory.
- **SC-007**: The data layer is usable without any viewer UI state or rendering
  dependency.

## Assumptions

- Library paths are stored exactly as provided by the scanner/import workflow,
  with path normalization decisions handled by that workflow.
- Comic hashes represent content identity and are supplied by the importer; hash
  algorithm selection is outside this feature.
- `metadata_id` may be empty until a metadata storage model is introduced or
  linked by a later feature.
- This feature covers ZIP and RAR archive readers only; PDF archive/page support
  remains a later VFS extension.
- Page entries are image-like files inside archives; non-image files and hidden
  metadata directories are excluded from page listing.
- The data layer will be called from background work in later UI features, so
  this specification avoids UI-facing behavior beyond preserving that boundary.
