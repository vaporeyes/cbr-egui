<!--
Sync Impact Report
Version change: template -> 1.0.0
Modified principles:
- Template principle 1 -> I. Decoupled Library and Viewer Boundaries
- Template principle 2 -> II. Archive-Native Virtual File System
- Template principle 3 -> III. Responsive UI Through Background Work
- Template principle 4 -> IV. Bounded Page Memory and Texture Caches
- Template principle 5 -> V. High-Fidelity Rendering With Graceful Failure
Added sections:
- Technology Constraints
- Development Workflow and Quality Gates
Removed sections:
- None
Templates requiring updates:
- ✅ .specify/templates/plan-template.md
- ✅ .specify/templates/spec-template.md
- ✅ .specify/templates/tasks-template.md
- ✅ .specify/templates/commands/*.md (no command templates present)
- ✅ AGENTS.md (already delegates project specifics to current plan)
Follow-up TODOs:
- None
-->
# Comic Book Reader in Rust + egui Constitution

## Core Principles

### I. Decoupled Library and Viewer Boundaries
The application MAY ship as a single binary, but library management and reading
MUST remain logically separate. LibraryService owns metadata, SQLite access,
file watching, archive discovery, reading progress, and bookmarks. ViewerState
owns canvas state, page navigation, page buffers, texture handles, zoom,
rotation, and reading-mode interaction. Cross-boundary communication MUST use
explicit data structures or messages, not shared mutable implementation details.

Rationale: YACReader's library/viewer separation keeps indexing, persistence,
and viewing concerns independently testable while allowing a simpler single
desktop executable.

### II. Archive-Native Virtual File System
Comic content MUST be read through a virtual file system abstraction that can
enumerate and stream pages from `.cbz`, `.cbr`, and `.pdf` sources without
extracting payloads to disk. CBZ MUST use native ZIP parsing. CBR MUST use an
approved RAR backend such as `compress-tools` or `unrar` FFI. PDF MUST use a
PDF renderer such as `pdfium-render`. Temporary full-archive extraction is not
allowed for normal reading paths.

Rationale: Archive-native access preserves disk space, avoids stale extracted
files, and provides one contract for page ordering, hidden-directory filtering,
and archive error handling.

### III. Responsive UI Through Background Work
The egui render loop MUST NOT perform disk I/O, archive decompression, image
decoding, PDF rendering, SQLite scans, or image resizing. CPU-bound and I/O-bound
reader work MUST run on background workers using `std::thread` and
`crossbeam_channel`, with tokio reserved only for future network sync or server
mode. Features that affect reading MUST preserve interactive frame delivery
with a target of 60 FPS during page navigation and zoom.

Rationale: Immediate-mode UI makes state flow simple, but blocking the main
thread immediately appears as stutter and missed input.

### IV. Bounded Page Memory and Texture Caches
Page caches MUST be explicitly bounded. The viewer MUST maintain a strict LRU
policy for decoded pages and `egui::TextureHandle` values, sized for the current
page, nearby previous pages, and a small prefetch window only. A feature MUST
justify any cache capacity above 10 full pages or any unbounded collection of
decoded image data. Plans that load entire chapters or archives into RAM are
constitution violations.

Rationale: A single 4K comic page can occupy tens of megabytes uncompressed; a
naive 100-page load can exhaust RAM or VRAM.

### V. High-Fidelity Rendering With Graceful Failure
Images MUST be downsampled asynchronously before GPU upload using high-quality
filters such as Lanczos or bicubic when source dimensions exceed display needs.
Prefetching SHOULD decode adjacent pages predictively, but it MUST respect cache
limits and MUST NOT block user input. Corrupt pages, malformed images, invalid
PDF pages, unsupported archive entries, and hidden metadata directories such as
`__MACOSX` MUST be handled without panics; the viewer MUST render an explicit
recoverable error page and allow navigation to continue.

Rationale: Readers need sharp pages, controlled VRAM use, and reliable progress
through imperfect real-world comic archives.

## Technology Constraints

The project uses Rust 2024 and `egui` through `eframe` for the desktop GUI.
State owned by the render loop MUST stay small and cheap to inspect each frame.

The library index, reading progress, and bookmarks MUST use `rusqlite`. SQLite
access MUST be mediated by LibraryService or a storage component owned by it,
not by viewer code.

Archive and media support MUST use maintained crates or FFI bindings selected
per format: `zip` for CBZ, `compress-tools` or `unrar` FFI for CBR,
`pdfium-render` for PDF, and `image` for bitmap decoding and resizing.

Concurrency MUST default to `std::thread` plus `crossbeam_channel` for decode,
archive, and resize workers. Introducing tokio requires a plan entry explaining
the network or server-mode requirement that makes async runtime ownership
necessary.

## Development Workflow and Quality Gates

Every feature plan MUST document how it preserves the LibraryService and
ViewerState boundary, how archive access flows through the VFS abstraction, and
which work is kept off the egui thread.

Every feature touching page loading, rendering, or navigation MUST include
bounded cache behavior, prefetch behavior, and failure behavior for corrupt or
unsupported pages. Performance-sensitive features MUST state measurable frame,
latency, RAM, or VRAM expectations and include verification steps appropriate to
the implementation stage.

Tests MUST cover pure ordering, filtering, cache eviction, service contracts, and
error handling where those behaviors can be exercised without a GUI. Manual or
automated quickstart validation MUST cover at least one readable archive and one
recoverable failure path for user-facing reader changes.

## Governance

This constitution supersedes conflicting local conventions, plans, templates,
and implementation shortcuts. Feature specifications, implementation plans,
tasks, and reviews MUST check compliance before work begins and again before a
feature is considered complete.

Amendments require an explicit constitution update, a Sync Impact Report, and
updates to affected Spec Kit templates or runtime guidance. Versioning follows
semantic versioning: MAJOR for removed or redefined principles, MINOR for added
principles or materially expanded governance, and PATCH for clarifications that
do not change obligations.

Code review MUST reject changes that perform reader I/O or decoding on the egui
thread, bypass LibraryService for library persistence, bypass the VFS for normal
archive reads, introduce unbounded page memory, or panic on corrupt content.
Exceptions require documented complexity tracking in the implementation plan and
must remain temporary.

**Version**: 1.0.0 | **Ratified**: 2026-05-13 | **Last Amended**: 2026-05-13
