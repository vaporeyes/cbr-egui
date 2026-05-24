# Implementation Plan: State Persistence & Lifecycle Management

**Branch**: `007-state-persistence-lifecycle` | **Date**: 2026-05-14 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/007-state-persistence-lifecycle/spec.md`

## Summary

Connect existing configuration and reading-progress APIs to the desktop app lifecycle. The app wrapper will own `AppConfig`, a config path, the `LibraryService`, settings window state, and a non-blocking status channel for persistence errors. Startup will load configuration before first render, initialize visuals and reader defaults, populate library state from the service, and attempt to resume the last valid reading session. The `eframe::App::save` hook will serialize configuration and flush active reading progress through LibraryService. A toolbar settings window will let users update theme, zoom sensitivity, and default reading direction without disturbing active reader state.

## Technical Context

**Language/Version**: Rust 2024  
**Primary Dependencies**: eframe/egui for desktop UI lifecycle and settings window; rusqlite through LibraryService for reading progress; serde/serde_json for configuration; existing viewer `ReadingDirection`; existing decode/prefetch workers remain unchanged.  
**Storage**: JSON config file under the app config path; rusqlite progress table through LibraryService/storage.  
**Testing**: `cargo test` with focused app/config/storage integration tests plus manual reader validation; `cargo clippy --all-targets -- -D warnings`.  
**Target Platform**: Desktop app.  
**Project Type**: Single-binary desktop app with logical LibraryService and ViewerState boundaries.  
**Performance Goals**: Save lifecycle must complete without visible reader stutter; startup resume with a valid local library should reach reader state in under 2 seconds; settings changes must apply immediately.  
**Constraints**: ViewerState must not access config files or SQLite; LibraryService remains owner of progress persistence; no archive/decode work is added to the save hook; no unbounded image/page cache changes.  
**Scale/Scope**: Single local user profile; one active reading session; existing library items and progress records; missing/unavailable comics fall back to Library.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Library/Viewer boundary**: PASS. Progress writes and resume reads go through LibraryService. ViewerState only receives derived preferences such as reading direction and zoom sensitivity.
- **Archive-native VFS**: PASS. This feature does not alter archive access; startup resume opens an existing comic through normal app routing and reader loading remains VFS-backed.
- **UI responsiveness**: PASS. The save hook writes small config/progress records only and does not perform archive reads, image decoding, PDF rendering, or scans. Existing background workers remain responsible for thumbnails and page decoding.
- **Bounded memory**: PASS. No page, decoded image, or texture cache capacity changes are introduced.
- **Rendering and failure paths**: PASS. Existing corrupt-page and prefetch behavior remains unchanged. Settings changes may refresh visuals but do not alter page decoding behavior.

## Project Structure

### Documentation (this feature)

```text
specs/007-state-persistence-lifecycle/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── lifecycle-settings.md
└── tasks.md              # Created by /speckit-tasks, not this phase
```

### Source Code (repository root)

```text
src/
├── main.rs               # Load config/service and construct app with startup state
├── config.rs             # AppConfig load/save defaults and validation
├── app/
│   ├── mod.rs            # App state, ReadingSession, resume/progress helpers
│   └── ui.rs             # EguiComicReaderApp lifecycle, toolbar, settings window
├── library/
│   ├── service.rs        # Progress and last-session service boundary
│   └── storage.rs        # SQLite progress persistence
└── viewer/
    ├── state.rs          # Viewer defaults consumed from config
    └── spread.rs         # ReadingDirection

tests/
├── app_routing.rs        # Lifecycle save, resume startup, settings state behavior
├── config.rs             # Config load/save/default validation
└── library_scanner.rs    # Existing storage/service coverage as needed
```

**Structure Decision**: Keep persistence orchestration in `app/ui.rs` and `main.rs`, with data writes routed through `config.rs` and `LibraryService`. Do not introduce a new subsystem unless implementation shows duplicated lifecycle logic that cannot stay local to `EguiComicReaderApp`.

## Complexity Tracking

No constitution violations or added complexity exceptions.

## Phase 0: Research

See [research.md](./research.md). All technical unknowns are resolved with existing project APIs and crate capabilities.

## Phase 1: Design & Contracts

See [data-model.md](./data-model.md), [contracts/lifecycle-settings.md](./contracts/lifecycle-settings.md), and [quickstart.md](./quickstart.md).

## Post-Design Constitution Check

- **Library/Viewer boundary**: PASS. Data model assigns config/progress ownership to app/config/service layers; ViewerState remains a consumer of preferences.
- **Archive-native VFS**: PASS. Contracts do not add archive access outside current reader loading.
- **UI responsiveness**: PASS. Quickstart and contracts require no archive/decode/scan work in save or settings paths.
- **Bounded memory**: PASS. No new caches are introduced.
- **Rendering and failure paths**: PASS. Invalid config and unavailable sessions have explicit fallback behavior; existing page failure behavior is preserved.
