# Implementation Plan: Domain Models & Data Layer

**Branch**: `HEAD` | **Date**: 2026-05-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-domain-data-layer/spec.md`

**Note**: Git setup reported no current commit, so the setup script returned
`HEAD` instead of a feature branch name. The active feature directory is
`specs/001-domain-data-layer`.

## Summary

Build the foundation data layer for the comic reader before UI work: persistent
library records for comics, folders, and progress; recoverable ComicInfo.xml
metadata extraction; and archive-native ZIP/RAR page readers behind a common VFS
interface with natural page ordering and hidden/non-page filtering. The
implementation will introduce LibraryService-owned SQLite storage and VFS-owned
archive access without depending on ViewerState or rendering code.

## Technical Context

**Language/Version**: Rust 2024  
**Primary Dependencies**: `rusqlite` for SQLite persistence, `zip` for CBZ/ZIP
archives, `7z` command backend for CBR/RAR access, `quick-xml` plus
`serde` for ComicInfo.xml parsing, `natord` or an equivalent natural comparator,
`thiserror` for recoverable domain errors, `tempfile` for archive/storage tests  
**Storage**: SQLite through `rusqlite`, mediated by LibraryService-owned storage
modules; no viewer-side database access  
**Testing**: `cargo test`; focused integration tests for schema initialization,
progress upsert, metadata parsing failures, ZIP page ordering/retrieval, and RAR
contract behavior when a supported backend is available  
**Target Platform**: Desktop app crate; data-layer logic is platform-neutral
except for selected RAR backend availability; this implementation uses `7z` on
`PATH` for CBR/RAR listing and entry streaming  
**Project Type**: Single Rust binary for now, organized into logical modules
with LibraryService and VFS boundaries  
**Performance Goals**: Initialize/query at least 1,000 comic records without
data loss; list archives without reading all page payloads; retrieve page bytes
on demand; keep all archive enumeration, decompression, metadata parsing, and
SQLite scans callable from background workers  
**Constraints**: No normal-path full archive extraction to disk; no egui or
ViewerState dependency in library/vfs modules; hidden metadata directories and
non-image entries are excluded; malformed ComicInfo.xml and corrupt/missing
pages return recoverable errors  
**Scale/Scope**: ZIP and RAR comic archives, ComicInfo.xml target fields only,
library schema for comics/folders/progress, and interfaces suitable for later
background import/viewer workflows

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Library/Viewer boundary**: PASS. Library storage and metadata parsing live
  under `src/library`; archive page access lives under `src/vfs`; no ViewerState
  or egui state is introduced for this feature.
- **Archive-native VFS**: PASS. ZIP and RAR readers implement a shared archive
  interface and stream requested page bytes from the archive backend without
  normal full-archive extraction.
- **UI responsiveness**: PASS. This phase adds synchronous data-layer APIs that
  are explicitly suitable for future background workers; no render-loop calls or
  UI dependencies are introduced.
- **Bounded memory**: PASS. Page listing stores paths and metadata only; page
  payloads are read only for a requested path. Decoded image/texture caches are
  outside this feature and remain unimplemented.
- **Rendering and failure paths**: PASS. Rendering/downsampling is outside this
  feature, but corrupt pages, malformed metadata, hidden metadata directories,
  and unsupported entries are represented as recoverable data-layer errors.

## Project Structure

### Documentation (this feature)

```text
specs/001-domain-data-layer/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── domain-data-layer.md
└── tasks.md              # Created later by /speckit-tasks
```

### Source Code (repository root)

```text
src/
├── main.rs
├── library/
│   ├── mod.rs
│   ├── models.rs         # Comic, Folder, Progress, ComicMetadata
│   ├── service.rs        # LibraryService boundary
│   ├── storage.rs        # rusqlite schema and repositories
│   └── metadata.rs       # ComicInfo.xml parser
└── vfs/
    ├── mod.rs
    ├── archive.rs        # ArchiveReader trait, ArchivePage, errors
    ├── zip.rs            # ZIP/CBZ reader
    ├── rar.rs            # RAR/CBR reader
    └── ordering.rs       # image filtering and natural page sorting

tests/
├── library_storage.rs
├── metadata_parser.rs
└── archive_vfs.rs
```

**Structure Decision**: Keep this feature inside the existing single crate and
introduce only `library` and `vfs` module trees. Viewer, decode, and cache
modules are deferred until later features need rendering behavior.

## Phase 0 Research

Completed in [research.md](./research.md). Decisions resolve dependency and
backend choices for SQLite schema ownership, ComicInfo.xml parsing, ZIP/RAR
readers, natural sorting, and recoverable errors.

## Phase 1 Design

Completed artifacts:

- [data-model.md](./data-model.md)
- [contracts/domain-data-layer.md](./contracts/domain-data-layer.md)
- [quickstart.md](./quickstart.md)

## Constitution Check: Post-Design

- **Library/Viewer boundary**: PASS. Data model and contracts expose
  LibraryService and VFS APIs only.
- **Archive-native VFS**: PASS. Contracts require archive-reader
  implementations for ZIP and RAR and forbid normal full extraction.
- **UI responsiveness**: PASS. Quickstart and contracts keep operations
  callable from background work and do not introduce UI code.
- **Bounded memory**: PASS. Contracts require listing paths separately from
  on-demand byte retrieval.
- **Rendering and failure paths**: PASS. Error contracts cover malformed
  metadata, missing/corrupt pages, unsupported entries, and archive backend
  failures as recoverable results.

## Complexity Tracking

No constitution violations.
