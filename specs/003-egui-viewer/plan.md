# Implementation Plan: egui Viewer Implementation

**Branch**: `HEAD` | **Date**: 2026-05-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/003-egui-viewer/spec.md`

**Note**: Git setup reported no current commit, so the setup script returned
`HEAD` instead of the feature branch name. The active feature directory is
`specs/003-egui-viewer`.

## Summary

Build the first usable reading surface on top of the existing data layer and
asynchronous image pipeline: a single-page `egui` viewer that displays the
current prepared texture, computes fit/fill sizes on resize, supports bounded
zoom and pan interaction, and resets zoom/pan state when the page changes. The
implementation will keep heavy page preparation out of the UI interaction path
and constrain this feature to single-page viewing.

## Technical Context

**Language/Version**: Rust 2024  
**Primary Dependencies**: existing `eframe`/`egui`, `image`,
`crossbeam-channel`, `lru`, `zip`, `rusqlite`, `quick-xml`, `serde`, `natord`,
`thiserror`, and `tempfile`  
**Storage**: No new persistent storage; the viewer consumes prepared texture
handles and display metadata from in-memory ViewerState/cache  
**Testing**: `cargo test`; focused tests for fit/fill aspect-ratio math,
zoom clamping, pan bounds, reset-on-page-change, and recoverable missing-page
presentation; manual/screenshot validation after rendering integration exists  
**Target Platform**: Desktop egui application  
**Project Type**: Single Rust binary/library crate with logical LibraryService,
VFS, decode/cache, and ViewerState boundaries  
**Performance Goals**: Preserve 60 FPS interaction target; zoom, pan, resize,
and page-change handlers must not perform archive I/O, image decoding, image
resizing, SQLite scans, or texture upload; display a prepared page within one
egui update after it is available  
**Constraints**: Single-page mode only; default fit shows the full page; fill
mode may crop only when explicitly selected; zoom resets on page identity
change; zoom/pan state remains small and cheap to inspect each frame; texture
handles are supplied by the async image pipeline/cache  
**Scale/Scope**: Single current page display, fit/fill layout, zoom/pan input
state, missing/loading/error presentation, and minimal non-obstructive chrome;
two-page spread mode, continuous scrolling, library navigation, and new decode
work are out of scope

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Library/Viewer boundary**: PASS. ViewerState owns canvas, current page,
  texture handles, zoom, pan, and reading interaction; LibraryService remains
  outside this feature.
- **Archive-native VFS**: PASS. The viewer consumes prepared page resources from
  the async pipeline and does not read archive bytes directly.
- **UI responsiveness**: PASS. Interaction handlers update only small viewer
  state and layout math. No disk I/O, decompression, image decoding, resizing,
  texture upload, PDF rendering, or SQLite scan is done during pointer/scroll
  handling.
- **Bounded memory**: PASS. The viewer uses existing bounded texture cache
  behavior from the async pipeline and introduces no new unbounded image
  collections.
- **Rendering and failure paths**: PASS. Missing/unavailable textures produce a
  recoverable loading or error presentation. Existing async decode/downsampling
  remains responsible for image preparation before texture upload.

## Project Structure

### Documentation (this feature)

```text
specs/003-egui-viewer/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── egui-viewer.md
└── tasks.md              # Created later by /speckit-tasks
```

### Source Code (repository root)

```text
src/
├── lib.rs
├── main.rs
├── library/              # Existing LibraryService and persistence
├── vfs/                  # Existing archive-native page byte access
├── decode/               # Existing background decode pipeline
├── cache/                # Existing bounded page texture cache
└── viewer/
    ├── mod.rs
    ├── prefetch.rs       # Existing prefetch scheduling
    ├── state.rs          # ViewerState, page identity, mode, zoom/pan state
    ├── layout.rs         # aspect-ratio fit/fill and pan bounds math
    └── ui.rs             # egui CentralPanel/ScrollArea rendering integration

tests/
├── viewer_layout.rs
├── viewer_interaction.rs
├── prefetch_scheduler.rs
└── page_cache.rs
```

**Structure Decision**: Extend the existing `viewer` module instead of creating
another UI subsystem. Keep pure layout and interaction math in testable modules;
keep egui rendering integration isolated in `viewer::ui`.

## Phase 0 Research

Completed in [research.md](./research.md). Decisions resolve fit/fill behavior,
zoom scale model, pan bounds, page-change reset, missing page presentation, and
UI chrome scope.

## Phase 1 Design

Completed artifacts:

- [data-model.md](./data-model.md)
- [contracts/egui-viewer.md](./contracts/egui-viewer.md)
- [quickstart.md](./quickstart.md)

## Constitution Check: Post-Design

- **Library/Viewer boundary**: PASS. Contracts require prepared page resources
  as input and keep archive/library access outside viewer state.
- **Archive-native VFS**: PASS. Viewer contracts do not expose raw archive
  access and depend on existing VFS/decode/cache flow.
- **UI responsiveness**: PASS. Contracts limit render-loop work to state,
  layout, polling prepared resources, and drawing.
- **Bounded memory**: PASS. Viewer does not add unbounded caches; texture
  retention remains delegated to the bounded page cache.
- **Rendering and failure paths**: PASS. Contracts include loading/error empty
  states, aspect-ratio safety, pan bounds, and reset behavior.

## Complexity Tracking

No constitution violations.
