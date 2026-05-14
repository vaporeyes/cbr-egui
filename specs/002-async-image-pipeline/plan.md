# Implementation Plan: Asynchronous Image Pipeline

**Branch**: `HEAD` | **Date**: 2026-05-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-async-image-pipeline/spec.md`

**Note**: Git setup reported no current commit, so the setup script returned
`HEAD` instead of the feature branch name. The active feature directory is
`specs/002-async-image-pipeline`.

## Summary

Add the reader image-preparation pipeline needed between archive page bytes and
display textures: a dedicated background decode worker pool, request/result
messages for raw bytes to decoded page images, smart prefetch scheduling for
`n+1`, `n+2`, and `n-1`, and a bounded LRU display cache that receives texture
handles created only on the main egui thread. The plan extends the single crate
with `decode`, `cache`, and `viewer` modules while keeping library/VFS ownership
separate.

## Technical Context

**Language/Version**: Rust 2024  
**Primary Dependencies**: existing `zip`, `rusqlite`, `quick-xml`, `serde`,
`natord`, `thiserror`, and `tempfile`; add `eframe`/`egui` for `ColorImage` and
`TextureHandle`, `image` for bitmap decoding/resizing, `crossbeam-channel` for
worker communication, and `lru` or an equivalent bounded LRU map for page and
texture cache policy  
**Storage**: No new persistent storage; page bytes continue to come from the
archive VFS, and display cache state stays in memory under ViewerState ownership  
**Testing**: `cargo test`; focused tests for decode request/result behavior,
recoverable decode failures, prefetch window ordering/deduplication, stale
result handling, and LRU eviction; manual validation of responsive navigation
with sample archives after UI integration exists  
**Target Platform**: Desktop egui application; worker and decode logic is
platform-neutral Rust except for egui texture creation on the main thread  
**Project Type**: Single Rust binary/library crate with logical LibraryService,
VFS, decode worker, cache, and ViewerState boundaries  
**Performance Goals**: Preserve 60 FPS interaction target by keeping archive
reads, image decoding, resizing, and prefetch scheduling side effects off the
egui render loop; process at least 25 queued decode requests without blocking
current-page state updates; cache capacity defaults to 5 pages and must not
exceed 10 pages in this feature  
**Constraints**: Decode workers may create `egui::ColorImage` but must not
create `egui::TextureHandle`; texture upload and cache insertion happen on the
main thread; all page bytes flow through VFS readers; prefetch order is
`n+1`, `n+2`, `n-1`; duplicate queued/in-flight/cached work is skipped; stale
results are ignored safely  
**Scale/Scope**: One in-memory decode pipeline for comic page images, bounded
decoded/texture retention for nearby pages, and no PDF rendering, network sync,
or persistent disk cache in this feature

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Library/Viewer boundary**: PASS. LibraryService remains responsible for
  persistent library data and VFS page-byte access. ViewerState owns current
  page, texture handles, display cache, and promotion of decode results.
- **Archive-native VFS**: PASS. The pipeline accepts raw page bytes supplied by
  VFS readers and does not introduce alternate archive access or extraction.
- **UI responsiveness**: PASS. CPU-heavy image decoding/resizing runs on
  dedicated `std::thread` workers communicating through `crossbeam_channel`;
  egui work is limited to polling results and texture creation on the main
  thread.
- **Bounded memory**: PASS. Cache capacity defaults to 5 full pages, supports a
  maximum of 10 pages for this feature, and uses strict LRU eviction. Prefetch
  window is limited to `n+1`, `n+2`, and `n-1`.
- **Rendering and failure paths**: PASS. Decode errors are recoverable; stale
  results can be ignored; downsampling is performed before GPU upload when the
  decode request includes target display bounds.

## Project Structure

### Documentation (this feature)

```text
specs/002-async-image-pipeline/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── async-image-pipeline.md
└── tasks.md              # Created later by /speckit-tasks
```

### Source Code (repository root)

```text
src/
├── lib.rs
├── main.rs
├── library/              # Existing LibraryService and metadata/storage logic
├── vfs/                  # Existing archive-native page byte access
├── decode/
│   ├── mod.rs
│   ├── error.rs          # DecodeError and worker lifecycle failures
│   ├── pipeline.rs       # raw bytes -> ColorImage decode/resizing
│   └── worker.rs         # WorkerPool and request/result channels
├── cache/
│   ├── mod.rs
│   └── page_cache.rs     # bounded LRU for decoded images/texture handles
└── viewer/
    ├── mod.rs
    └── prefetch.rs       # current-page prefetch scheduling and stale filtering

tests/
├── decode_pipeline.rs
├── prefetch_scheduler.rs
├── page_cache.rs
└── archive_vfs.rs        # Existing tests from previous feature
```

**Structure Decision**: Keep the feature in the existing crate. Add focused
`decode`, `cache`, and `viewer` modules; do not move existing library/VFS code.
The viewer module in this feature is limited to scheduling and cache ownership
contracts, not full UI rendering.

## Phase 0 Research

Completed in [research.md](./research.md). Decisions resolve worker runtime,
channel ownership, image decode/downsampling, prefetch deduplication, cache
capacity, texture promotion, and stale/failure handling.

## Phase 1 Design

Completed artifacts:

- [data-model.md](./data-model.md)
- [contracts/async-image-pipeline.md](./contracts/async-image-pipeline.md)
- [quickstart.md](./quickstart.md)

## Constitution Check: Post-Design

- **Library/Viewer boundary**: PASS. Data model and contracts keep VFS page-byte
  sourcing outside ViewerState while ViewerState owns texture cache and current
  page scheduling.
- **Archive-native VFS**: PASS. Contracts require callers to source bytes from
  `ArchiveReader` and pass raw bytes into decode requests.
- **UI responsiveness**: PASS. Contracts isolate decode/resize work to worker
  threads and keep main-thread work to nonblocking result polling and texture
  upload.
- **Bounded memory**: PASS. Cache contract enforces fixed capacity with LRU
  eviction and default capacity 5, maximum 10.
- **Rendering and failure paths**: PASS. Contracts include corrupt/unsupported
  bytes, worker failures, stale results, and texture-promotion failures as
  recoverable paths.

## Complexity Tracking

No constitution violations.
