# Implementation Plan: Library Metadata Surfacing

**Branch**: `008-library-metadata-surfacing` | **Date**: 2026-05-14 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/008-library-metadata-surfacing/spec.md`

## Summary

Surface existing ComicInfo metadata in the library by enriching `LibraryGridItem` with display metadata and filter keys, then add a session-scoped library filter that can narrow thumbnail and list views by series or containing folder. The implementation stays in the library/application shell: storage provides metadata-aware rows, `LibraryService` formats display-ready summaries and groups, and the egui library view renders a compact filter control before passing filtered items to the existing grid/list renderers.

## Technical Context

**Language/Version**: Rust 2024  
**Primary Dependencies**: eframe/egui, rusqlite, quick-xml metadata parsing already present, standard library path utilities  
**Storage**: rusqlite for library index and metadata, accessed through LibraryService/LibraryStorage  
**Testing**: cargo test with focused library storage/service and app routing tests; manual library scan validation  
**Target Platform**: desktop app  
**Project Type**: single-binary desktop app with logical LibraryService and ViewerState boundaries  
**Performance Goals**: filtering 500 library items should complete within 1 second from the user's perspective; no SQLite work during per-frame filter rendering  
**Constraints**: LibraryService remains the owner of metadata and SQLite access; ViewerState and archive VFS are not involved in this feature; UI filtering is in-memory over already-loaded library items  
**Scale/Scope**: large local comic libraries where grid/list browsing should remain responsive while thumbnails continue loading in the background

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Library/Viewer boundary**: PASS. Metadata joins, subtitle formatting, and grouping belong to LibraryService and library app state. ViewerState remains unchanged.
- **Archive-native VFS**: PASS. No new archive page access is introduced. Existing scan/metadata ingestion paths remain responsible for archive metadata extraction.
- **UI responsiveness**: PASS. SQLite reads happen during library hydration/scan reconciliation, not inside the render loop's per-item filtering. Filtering uses already-loaded vectors.
- **Bounded memory**: PASS. The feature adds small strings and grouping keys to library items only; no decoded page or texture cache capacity changes.
- **Rendering and failure paths**: PASS. Reader rendering is unchanged. Missing or malformed metadata falls back to existing title/folder display without blocking library browsing.

## Project Structure

### Documentation (this feature)

```text
specs/008-library-metadata-surfacing/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── library-metadata-ui.md
└── tasks.md
```

### Source Code (repository root)

```text
src/
├── app/
│   ├── mod.rs          # LibraryViewState filter state and in-memory filtering helpers
│   └── ui.rs           # Library filter controls, subtitle rendering in grid/list
├── library/
│   ├── models.rs       # LibraryGridItem metadata fields and group/filter models
│   ├── service.rs      # metadata-aware library item assembly and group derivation
│   └── storage.rs      # metadata LEFT JOIN row query
└── viewer/             # unchanged

tests/
├── app_routing.rs      # filter state and visible-item behavior
├── library_storage.rs  # metadata join coverage
└── metadata_parser.rs  # existing parser coverage remains unchanged
```

**Structure Decision**: Keep the feature inside existing `app` and `library` modules. Storage performs metadata joins, service maps rows into display/group data, app state holds the active filter, and UI renders the controls plus filtered grid/list items. No new crate or viewer module is needed.

## Complexity Tracking

No constitution violations are expected.
