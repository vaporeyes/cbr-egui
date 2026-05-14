# Implementation Plan: Library Spread UI

**Branch**: `HEAD` | **Date**: 2026-05-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/004-library-spread-ui/spec.md`

**Note**: Git setup reported no current commit, so the setup script returned
`HEAD` even though the specification hook generated `004-library-spread-ui`.
The active feature directory is `specs/004-library-spread-ui`.

## Summary

Add the first navigational application shell around the existing library,
archive, decode, cache, and viewer layers. The reader gains an optional
two-page spread mode that uses page dimensions to decide whether to render one
pre-stitched landscape page or a current-plus-next portrait pair. The library
gains root-folder scanning, filesystem change monitoring, cover thumbnail
generation/caching, and a responsive cover grid that routes into the reader.

## Technical Context

**Language/Version**: Rust 2024  
**Primary Dependencies**: existing `eframe`/`egui`, `rusqlite`,
`crossbeam-channel`, `zip`, `image`, `lru`, `natord`, `quick-xml`, `serde`,
`tempfile`, `thiserror`; add `notify` for filesystem watching and a local
application-data path helper if needed for thumbnail cache placement  
**Storage**: Existing `rusqlite` library database for folders/comics/progress;
extend schema for availability and thumbnail metadata if needed. Thumbnail
files are stored in a bounded local disk cache outside source archive folders.  
**Testing**: `cargo test` with focused integration tests for spread decisions,
library scanning/watcher event coalescing, thumbnail cache invalidation, grid
layout sizing, and app-state routing. Manual quickstart validates one readable
archive, a recoverable corrupt/missing cover, and library-to-reader navigation.  
**Target Platform**: Desktop `eframe` application  
**Project Type**: Single Rust binary/library crate with logical boundaries:
`LibraryService` for collection persistence/watching and `ViewerState` for
reading canvas state  
**Performance Goals**: Preserve 60 FPS target for egui interaction. Spread
toggle and page turns should update visible layout within one egui update when
page resources are already prepared. File scanning, archive reads, thumbnail
generation, image decoding/resizing, and SQLite scans stay off the egui render
loop. New/removed archives should appear or be marked unavailable within 5
seconds after local filesystem events settle.  
**Constraints**: Archive content stays behind the VFS abstraction with no
normal reading-path extraction. Decoded page/texture caches remain bounded to
the existing 5-page default and 10-page maximum. Cover thumbnails are capped at
300px display height and cached on disk with source-change invalidation.  
**Scale/Scope**: 500-comic local library target with responsive grid browsing.
Two-page spread mode pairs the current page with the immediately following page
only; manga direction, custom pairing offsets, search/filtering, multi-root
management, and advanced library editing are out of scope.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Library/Viewer boundary**: PASS. Library root scanning, filesystem
  watching, SQLite writes, comic availability, and thumbnail metadata stay in
  `LibraryService`/library-owned modules. `ViewerState` owns spread toggle,
  current display decision, page statuses, textures, zoom, pan, and reading
  interaction. Cross-boundary data is explicit comic/page resource messages.
- **Archive-native VFS**: PASS. Page dimensions, cover bytes, and reader page
  bytes are obtained through archive reader/VFS contracts. Thumbnail files are
  derived cache artifacts only; source archive payloads are not extracted into
  ordinary folders for reading.
- **UI responsiveness**: PASS. The egui update loop polls prepared state and
  draws grids/pages. Directory scans, watch handling, archive reads, image
  decoding/resizing, thumbnail writes, and SQLite scans run through background
  workers and service methods outside direct render-loop interaction.
- **Bounded memory**: PASS. Reader decoded page and texture cache capacity
  remains within the existing 5-10 page policy. Spread mode may request the
  current and next page plus existing adjacent prefetch candidates but does not
  load an entire chapter. Thumbnail images are downsampled before display and
  persisted on disk to avoid retaining full cover sets in RAM.
- **Rendering and failure paths**: PASS. Page and thumbnail generation use
  existing high-quality async resize/decode behavior. Corrupt archives, missing
  next pages, unsupported images, stale thumbnails, and inaccessible roots
  produce recoverable UI states and do not panic.

## Project Structure

### Documentation (this feature)

```text
specs/004-library-spread-ui/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── library-shell.md
│   └── spread-viewer.md
└── tasks.md              # Created by /speckit-tasks
```

### Source Code (repository root)

```text
src/
├── main.rs
├── app/
│   ├── mod.rs            # AppState and top-level egui routing
│   └── ui.rs             # library/reader shell composition
├── library/
│   ├── mod.rs
│   ├── service.rs        # collection API remains the persistence boundary
│   ├── storage.rs        # SQLite schema/query extensions
│   ├── scanner.rs        # root scan and archive discovery
│   ├── watcher.rs        # filesystem watch event coalescing
│   └── thumbnails.rs     # cover thumbnail disk-cache metadata
├── viewer/
│   ├── mod.rs
│   ├── layout.rs         # existing geometry plus spread layout helpers
│   ├── state.rs          # spread toggle/status additions
│   ├── spread.rs         # pure spread decision rules
│   └── ui.rs             # single/spread rendering composition
├── vfs/
│   ├── mod.rs
│   └── archive.rs        # archive reader contract extensions if dimensions needed
├── decode/
│   └── ...               # existing background decode/resize workers
└── cache/
    └── ...               # existing bounded LRU cache policy

tests/
├── app_routing.rs
├── library_scanner.rs
├── library_watcher.rs
├── thumbnail_cache.rs
├── spread_viewer.rs
└── viewer_layout.rs
```

**Structure Decision**: Add a small `app` module for navigation state instead
of mixing library shell behavior into `main.rs` or `viewer`. Keep spread
decision math pure in `viewer::spread`, library scan/watch/thumbnail behavior
inside `library`, and image/archive work behind existing VFS/decode boundaries.

## Complexity Tracking

No constitution violations.

## Constitution Check: Post-Design

- **Library/Viewer boundary**: PASS. `data-model.md` assigns scanning,
  watching, collection records, and thumbnail cache metadata to library-owned
  entities while `ReadingSession` and `SpreadDecision` remain reader state.
- **Archive-native VFS**: PASS. Contracts require cover and page preparation to
  use existing archive access and prohibit source archive extraction into normal
  library folders.
- **UI responsiveness**: PASS. Contracts specify that app/viewer rendering paths
  consume prepared state and route commands to workers/services instead of
  performing storage, archive, decode, resize, or thumbnail work directly.
- **Bounded memory**: PASS. The plan keeps page/texture cache limits at the
  existing 5-10 page policy and moves large cover persistence to disk-backed
  thumbnails with bounded display size.
- **Rendering and failure paths**: PASS. Contracts and quickstart cover missing
  next pages, stale worker results, corrupt archives, missing cover pages,
  watcher bursts, inaccessible roots, and thumbnail write failures.
