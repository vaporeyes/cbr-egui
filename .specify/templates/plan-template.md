# Implementation Plan: [FEATURE]

**Branch**: `[###-feature-name]` | **Date**: [DATE] | **Spec**: [link]
**Input**: Feature specification from `/specs/[###-feature-name]/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

[Extract from feature spec: primary requirement + technical approach from research]

## Technical Context

<!--
  ACTION REQUIRED: Replace the content in this section with the technical details
  for the project. The structure here is presented in advisory capacity to guide
  the iteration process.
-->

**Language/Version**: Rust 2024 or NEEDS CLARIFICATION  
**Primary Dependencies**: eframe/egui, rusqlite, crossbeam_channel, zip, image,
compress-tools or unrar FFI, pdfium-render, or NEEDS CLARIFICATION  
**Storage**: rusqlite for library index, reading progress, and bookmarks  
**Testing**: cargo test plus focused integration/manual reader validation  
**Target Platform**: desktop app or NEEDS CLARIFICATION
**Project Type**: single-binary desktop app with logical LibraryService and ViewerState boundaries  
**Performance Goals**: 60 FPS target for egui interaction; no reader I/O,
archive work, image decoding, PDF rendering, SQLite scans, or resizing on the UI thread  
**Constraints**: archive-native VFS for CBZ/CBR/PDF; strict LRU page/texture
cache, default capacity 5-10 pages; async high-quality downsampling before GPU upload  
**Scale/Scope**: large comic libraries and high-resolution archives without
loading whole chapters into RAM

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Library/Viewer boundary**: Does the plan keep LibraryService responsible for
  metadata, file watching, SQLite, progress, and bookmarks while ViewerState owns
  canvas, navigation, page buffers, textures, zoom, and reading interaction?
- **Archive-native VFS**: Does all CBZ/CBR/PDF page access go through a VFS
  abstraction without normal-path extraction to disk?
- **UI responsiveness**: Are disk I/O, decompression, image decoding, PDF
  rendering, SQLite scans, and resizing kept off the egui render loop using
  `std::thread` and `crossbeam_channel` unless tokio is justified for network/server mode?
- **Bounded memory**: Does the plan specify decoded-page and texture LRU limits,
  prefetch window size, and why the cache cannot exceed 10 full pages if it does?
- **Rendering and failure paths**: Does the plan define async Lanczos/bicubic
  downsampling before GPU upload and recoverable handling for corrupt pages,
  malformed images, hidden metadata directories, and unsupported entries?

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/           # Phase 1 output (/speckit-plan command)
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)
<!--
  ACTION REQUIRED: Replace the placeholder tree below with the concrete layout
  for this feature. Delete unused options and expand the chosen structure with
  real paths (e.g., apps/admin, packages/something). The delivered plan must
  not include Option labels.
-->

```text
src/
├── main.rs
├── library/            # LibraryService: metadata, rusqlite, watching, progress
├── viewer/             # ViewerState: egui canvas, navigation, page textures
├── vfs/                # CBZ/CBR/PDF archive abstraction and page ordering
├── decode/             # background decode, resize, prefetch workers
└── cache/              # decoded page and texture LRU policy

tests/
├── integration/
└── unit/
```

**Structure Decision**: [Document the selected structure and reference the real
directories captured above]

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| [e.g., 4th project] | [current need] | [why 3 projects insufficient] |
| [e.g., Repository pattern] | [specific problem] | [why direct DB access insufficient] |
