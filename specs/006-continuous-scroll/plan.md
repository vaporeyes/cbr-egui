# Implementation Plan: Continuous Vertical Scroll

**Branch**: `006-continuous-scroll` | **Date**: 2026-05-14 | **Spec**: [spec.md](./spec.md)  
**Input**: Feature specification from `/specs/006-continuous-scroll/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

Add a continuous vertical reader mode that lays pages out in one virtual scroll column, renders only the pages intersecting the viewport plus a one-page overdraw margin, and uses stable placeholder measurements until decoded page dimensions are known. The implementation will extend existing viewer layout helpers for pure geometry, keep page loading and decoding on existing background workers, upload textures only on the egui thread, and preserve the existing bounded display cache.

## Technical Context

**Language/Version**: Rust 2024  
**Primary Dependencies**: eframe/egui, crossbeam_channel, image, lru, zip, unrar, pdfium-render, rusqlite  
**Storage**: rusqlite remains limited to library index, reading progress, and bookmarks; continuous scroll measurements and visible-window state are transient active-reader state  
**Testing**: cargo test with focused geometry, app routing, cache, and viewer interaction tests; manual reader validation with large multi-page archives and corrupt-page cases  
**Target Platform**: Desktop app  
**Project Type**: Single-binary desktop app with logical LibraryService and ViewerState boundaries  
**Performance Goals**: Maintain responsive reader input with no noticeable freezes over 100 ms during rapid scroll; steady-state continuous layout prepares only visible pages plus one page above and below; cache remains bounded to the existing maximum of 10 full pages  
**Constraints**: No archive I/O, decompression, image decode, PDF render, SQLite scan, or resize work on the egui render loop; texture upload remains main-thread only; continuous mode must not force all pages in an archive into RAM or VRAM  
**Scale/Scope**: Active comic only; supports large comics with hundreds of pages, mixed page aspect ratios, corrupt pages, and window resizing without full-document texture residency

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Library/Viewer boundary**: PASS. Continuous scroll is reader canvas state. LibraryService remains responsible for discovery, persistence, and progress; ViewerState and ReadingSession own layout mode, page display state, measurements, cache, and decode coordination.
- **Archive-native VFS**: PASS. Near-visible page preparation uses the existing reader page-load path and VFS archive readers for CBZ/CBR/PDF; no normal-path extraction to disk is introduced.
- **UI responsiveness**: PASS. Geometry calculation and non-blocking result polling are allowed on the render loop; archive reads, image decode, PDF render, and resizing remain background work via `std::thread` and `crossbeam_channel`.
- **Bounded memory**: PASS. The visible-page window is bounded to viewport intersections plus one page of overdraw, and display resources continue to flow through `PageTextureCache` with the existing hard cap.
- **Rendering and failure paths**: PASS. Corrupt near-visible pages become recoverable placeholders; direct visible-page failures continue to use the existing fallback path and navigation remains possible.

## Project Structure

### Documentation (this feature)

```text
specs/006-continuous-scroll/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── continuous-scroll.md
└── tasks.md              # Created by /speckit-tasks, not /speckit-plan
```

### Source Code (repository root)

```text
src/
├── app/
│   ├── mod.rs            # ReadingSession owns transient continuous-scroll state
│   └── ui.rs             # Reader toolbar, mode routing, viewport-triggered loading
├── cache/
│   └── page_cache.rs     # Existing bounded display-resource LRU
├── decode/
│   ├── pipeline.rs       # Existing decode requests and cancellation
│   └── worker.rs         # Existing worker result polling/submission
├── viewer/
│   ├── layout.rs         # Page sizing helpers
│   ├── spread.rs         # Existing ReadingLayoutMode and continuous height helper
│   ├── state.rs          # ViewerState layout mode and page status
│   └── ui.rs             # Canvas rendering for paged and continuous modes
└── vfs/                  # Archive-native page reads for CBZ/CBR/PDF

tests/
├── app_routing.rs
├── page_cache.rs
├── spread_viewer.rs
├── viewer_interaction.rs
└── viewer_layout.rs
```

**Structure Decision**: Extend the current reader modules rather than creating a separate continuous-reader subsystem. Pure scroll geometry belongs in `viewer` so it can be tested without egui texture handles. Runtime coordination belongs in `ReadingSession`/`app/ui.rs` because those components already own the active comic, cache, worker pool, and egui context needed for texture upload. The library layer is not changed.

## Complexity Tracking

No constitution violations identified.

## Phase 0: Research

Research findings are captured in [research.md](./research.md). Key decisions:

- Model continuous scroll as a virtual document with measured or placeholder page rectangles and a one-page overdraw window.
- Keep texture residency bounded by reusing the existing page texture cache and dispatching work only for pages inside the near-visible window.
- Store page measurements independently of texture residency so evicted pages can still keep stable layout height.
- Treat two-page spread as paged-layout-only while continuous vertical mode is active.
- Preserve reader location on layout recalculation by anchoring to the nearest visible page and intra-page offset.

## Phase 1: Design & Contracts

Design artifacts:

- [data-model.md](./data-model.md)
- [contracts/continuous-scroll.md](./contracts/continuous-scroll.md)
- [quickstart.md](./quickstart.md)

## Post-Design Constitution Check

- **Library/Viewer boundary**: PASS. Data model keeps continuous scroll state under active reader/session/viewer ownership and does not add library persistence responsibilities.
- **Archive-native VFS**: PASS. Contract requires near-visible page loads to use existing reader archive access and VFS support.
- **UI responsiveness**: PASS. Contract limits render-loop work to geometry, cache checks, non-blocking result polling, and texture upload for completed background work.
- **Bounded memory**: PASS. Data model separates lightweight measurements from bounded display entries and requires only the visible overdraw window to be prepared.
- **Rendering and failure paths**: PASS. Failed page preparation produces recoverable placeholders and does not block surrounding pages.
