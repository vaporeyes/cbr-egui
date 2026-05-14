# Implementation Plan: Prefetch & VRAM Cache Integration

**Branch**: `005-prefetch-vram-cache` | **Date**: 2026-05-13 | **Spec**: [spec.md](./spec.md)  
**Input**: Feature specification from `/specs/005-prefetch-vram-cache/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

Integrate existing nearby-page prefetch math, background decode workers, cancellation tokens, and bounded texture cache into the reader app loop. The reader will dispatch best-effort background preparation for `[current + 1, current + 2, current - 1]`, reconcile completed background pages into a display-ready texture cache on the egui thread, use cached textures for page turns, and cancel stale in-flight work when navigation moves away from old candidates.

## Technical Context

**Language/Version**: Rust 2024  
**Primary Dependencies**: eframe/egui, crossbeam_channel, image, lru, zip, unrar, pdfium-render, rusqlite  
**Storage**: rusqlite remains limited to library index, reading progress, and bookmarks; this feature stores prefetch/cache state only in active reader memory  
**Testing**: cargo test, focused unit/integration tests for prefetch dispatch/reconciliation/cancellation, plus manual reader validation with CBZ/CBR/PDF where available  
**Target Platform**: Desktop app  
**Project Type**: Single-binary desktop app with logical LibraryService and ViewerState boundaries  
**Performance Goals**: Reader input remains responsive during page turns; adjacent prepared pages display within 100 ms; background request window remains bounded to current-page neighbors plus visible spread page  
**Constraints**: Archive reads and image decoding stay off the egui render loop for prefetch; texture upload and cache insertion occur only on the egui thread; cache capacity remains <= 10 full pages; stale prefetches are cancelled during large jumps  
**Scale/Scope**: Active comic only; large archives and high-resolution pages are supported without loading an entire chapter or archive into RAM/VRAM

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Library/Viewer boundary**: PASS. LibraryService remains responsible for library persistence and discovery. Reader prefetch state is scoped to `ReadingSession`, while `ViewerState` continues to own canvas/display state.
- **Archive-native VFS**: PASS. Page bytes for background requests will be read through the existing VFS readers for CBZ/CBR/PDF, with no normal-path extraction to disk.
- **UI responsiveness**: PASS. Background decode workers already use `std::thread` and `crossbeam_channel`; reconciliation performs only non-blocking polling and egui texture upload on the render loop.
- **Bounded memory**: PASS. Prefetch window is three nearby pages, and display textures are inserted into `PageTextureCache` with the existing hard maximum of 10 pages.
- **Rendering and failure paths**: PASS. Decode failures remain recoverable; corrupt prefetched pages are ignored or recorded until directly viewed, and visible corrupt pages use the existing fallback page.

## Project Structure

### Documentation (this feature)

```text
specs/005-prefetch-vram-cache/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── reader-prefetch.md
└── tasks.md              # Created by /speckit-tasks, not /speckit-plan
```

### Source Code (repository root)

```text
src/
├── app/
│   ├── mod.rs            # ReadingSession owns prefetch/cache state
│   └── ui.rs             # App update loop dispatches, reconciles, and uses cached pages
├── cache/
│   └── page_cache.rs     # Existing bounded texture LRU
├── decode/
│   ├── pipeline.rs       # Existing DecodeRequest, DecodePurpose, CancellationToken
│   └── worker.rs         # Existing WorkerPool polling/submission
├── viewer/
│   ├── prefetch.rs       # Existing candidate and stale-result helpers
│   └── state.rs          # ViewerState page status remains display state
└── vfs/                  # Archive-native page reads for CBZ/CBR/PDF

tests/
├── app_routing.rs
├── decode_pipeline.rs
├── page_cache.rs
└── prefetch_scheduler.rs
```

**Structure Decision**: Extend the existing reader modules instead of creating a new subsystem. `ReadingSession` will hold active prefetch metadata and the texture cache because those are scoped to the currently open comic. `app/ui.rs` will coordinate per-frame polling and navigation cache hits because it owns the egui context needed for texture creation. Pure helper behavior stays testable in `viewer/prefetch.rs` or new app-level helper functions.

## Complexity Tracking

No constitution violations identified.

## Phase 0: Research

Research findings are captured in [research.md](./research.md). Key decisions:

- Use the existing `WorkerPool` for both direct and prefetch decode requests, while keeping direct visible-page loads higher priority by checking cache first and limiting prefetch submissions.
- Track cancellation tokens per in-flight page, not globally, so rapid page jumps can cancel only stale neighbors.
- Reconcile worker results in the egui update loop because texture handles must be created on the main UI context.
- Keep the cache scoped to a reading session and bounded by the existing page texture cache maximum.

## Phase 1: Design & Contracts

Design artifacts:

- [data-model.md](./data-model.md)
- [contracts/reader-prefetch.md](./contracts/reader-prefetch.md)
- [quickstart.md](./quickstart.md)

## Post-Design Constitution Check

- **Library/Viewer boundary**: PASS. Data model keeps transient reader prefetch state inside `ReadingSession` and display state inside `ViewerState`.
- **Archive-native VFS**: PASS. Contract requires page byte reads through archive readers.
- **UI responsiveness**: PASS. Contracts require background decode and non-blocking result polling; texture upload remains main-thread only.
- **Bounded memory**: PASS. Data model and contract enforce a small neighbor window and bounded LRU cache.
- **Rendering and failure paths**: PASS. Failed and cancelled prefetches do not block current rendering, and direct navigation still uses existing recoverable page fallback.
