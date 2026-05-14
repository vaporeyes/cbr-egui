# Tasks: Library Spread UI

**Input**: Design documents from `/specs/004-library-spread-ui/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/library-shell.md, contracts/spread-viewer.md, quickstart.md

**Prerequisite Note**: `.specify/scripts/bash/check-prerequisites.sh --json`
reported `ERROR: Not on a feature branch. Current branch: HEAD` because the Git
repository has no current commit. Tasks are generated from the active feature
directory recorded in `.specify/feature.json`: `specs/004-library-spread-ui`.

**Tests**: Included because the implementation plan and contracts require
focused validation for spread decisions, watcher reconciliation, thumbnail
cache invalidation, grid layout behavior, and app-state routing.

**Organization**: Tasks are grouped by user story so each story can be
implemented and tested independently after shared foundations are complete.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel because it touches different files or depends
  only on completed prerequisites
- **[Story]**: User story label, required only for user story phases
- Every task includes an exact repository-relative file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the feature modules, test files, and dependencies needed by
spread reading, library synchronization, thumbnails, grid browsing, and app
routing.

- [X] T001 Add `notify` dependency for filesystem watching in Cargo.toml
- [X] T002 Create application shell module declarations in src/app/mod.rs
- [X] T003 Create application shell UI module placeholder in src/app/ui.rs
- [X] T004 Wire the app module through src/lib.rs
- [X] T005 Create spread decision module placeholder in src/viewer/spread.rs
- [X] T006 Wire spread module exports through src/viewer/mod.rs
- [X] T007 Create library scanner module placeholder in src/library/scanner.rs
- [X] T008 Create library watcher module placeholder in src/library/watcher.rs
- [X] T009 Create thumbnail cache module placeholder in src/library/thumbnails.rs
- [X] T010 Wire scanner, watcher, and thumbnails through src/library/mod.rs
- [X] T011 [P] Create spread viewer integration test file in tests/spread_viewer.rs
- [X] T012 [P] Create library scanner integration test file in tests/library_scanner.rs
- [X] T013 [P] Create library watcher integration test file in tests/library_watcher.rs
- [X] T014 [P] Create thumbnail cache integration test file in tests/thumbnail_cache.rs
- [X] T015 [P] Create app routing integration test file in tests/app_routing.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define shared models and service contracts that every story needs
without tying UI rendering directly to archive, decode, storage, or watch work.

**Critical**: No user story implementation should begin until this phase is
complete.

- [X] T016 Define AppState, LibraryViewState, and ReadingSession skeletons in src/app/mod.rs
- [X] T017 Define LibraryRoot, LibraryScanStatus, and ComicAvailability models in src/library/models.rs
- [X] T018 Define CoverThumbnail, ThumbnailStatus, and LibraryGridItem models in src/library/models.rs
- [X] T019 Define source fingerprint helper contract for archive paths in src/library/thumbnails.rs
- [X] T020 Extend LibraryStorage schema for comic availability and thumbnail metadata in src/library/storage.rs
- [X] T021 Extend LibraryService methods for listing comics and updating availability in src/library/service.rs
- [X] T022 Define recoverable library synchronization errors in src/library/errors.rs
- [X] T023 Define page resource identity and generation helpers for spread composition in src/viewer/spread.rs
- [X] T024 Add spread_mode_enabled and spread reset fields to ViewerState in src/viewer/state.rs
- [X] T025 Add two-page spread sizing helper declarations in src/viewer/layout.rs

**Checkpoint**: Shared app, library, thumbnail, and spread contracts are ready
for user story implementation.

---

## Phase 3: User Story 1 - Read With Optional Spreads (Priority: P1) MVP

**Goal**: Provide a reader toggle that composes either one pre-stitched
landscape page or a current-plus-next portrait spread without blocking the UI.

**Independent Test**: Open a comic fixture with portrait pages and a landscape
spread, toggle spread mode, and verify landscape pages render alone while
portrait pages pair with the next page when available.

### Tests for User Story 1

- [X] T026 [P] [US1] Add spread-disabled single-page decision tests in tests/spread_viewer.rs
- [X] T027 [P] [US1] Add pre-stitched landscape spread renders-alone tests in tests/spread_viewer.rs
- [X] T028 [P] [US1] Add portrait and square current-page pair decision tests in tests/spread_viewer.rs
- [X] T029 [P] [US1] Add last-page and missing-next-page fallback tests in tests/spread_viewer.rs
- [X] T030 [P] [US1] Add stale next-page result ignored tests in tests/spread_viewer.rs
- [X] T031 [P] [US1] Add spread composition reset zoom and pan tests in tests/viewer_interaction.rs
- [X] T032 [P] [US1] Add side-by-side spread layout sizing tests in tests/viewer_layout.rs

### Implementation for User Story 1

- [X] T033 [US1] Implement SpreadDecision and SpreadSideStatus types in src/viewer/spread.rs
- [X] T034 [US1] Implement pure spread decision rules in src/viewer/spread.rs
- [X] T035 [US1] Implement stale spread resource generation checks in src/viewer/spread.rs
- [X] T036 [US1] Implement side-by-side spread layout sizing helpers in src/viewer/layout.rs
- [X] T037 [US1] Implement ViewerState spread-mode toggle and composition reset behavior in src/viewer/state.rs
- [X] T038 [US1] Integrate spread decision recomputation with page status updates in src/viewer/state.rs
- [X] T039 [US1] Render paired ready pages side-by-side in src/viewer/ui.rs
- [X] T040 [US1] Render pending or failed next-page side states without hiding the current page in src/viewer/ui.rs
- [X] T041 [US1] Add spread-mode chrome toggle to the reader UI in src/viewer/ui.rs
- [X] T042 [US1] Extend prefetch candidates to include required next spread page in src/viewer/prefetch.rs

**Checkpoint**: User Story 1 is functional and testable with `cargo test --test spread_viewer --test viewer_layout --test viewer_interaction`.

---

## Phase 4: User Story 2 - Keep Library Current Automatically (Priority: P2)

**Goal**: Let users configure a root folder and keep library records in sync as
supported archives are added, removed, or changed.

**Independent Test**: Configure a temporary library root, add/remove supported
archive fixtures, and verify database-backed collection records reconcile
without duplicate comics.

### Tests for User Story 2

- [X] T043 [P] [US2] Add recursive supported archive discovery tests in tests/library_scanner.rs
- [X] T044 [P] [US2] Add hidden metadata and unsupported file filtering tests in tests/library_scanner.rs
- [X] T045 [P] [US2] Add add/remove/change reconciliation tests in tests/library_scanner.rs
- [X] T046 [P] [US2] Add duplicate path prevention tests in tests/library_scanner.rs
- [X] T047 [P] [US2] Add watcher event coalescing tests in tests/library_watcher.rs
- [X] T048 [P] [US2] Add watcher burst schedules full reconciliation tests in tests/library_watcher.rs
- [X] T049 [P] [US2] Add LibraryService availability update tests in tests/library_storage.rs

### Implementation for User Story 2

- [X] T050 [US2] Implement supported archive discovery and recursive root scanning in src/library/scanner.rs
- [X] T051 [US2] Implement stable source fingerprint calculation for scan reconciliation in src/library/scanner.rs
- [X] T052 [US2] Implement scanner-to-ComicInput mapping and page count probing through VFS in src/library/scanner.rs
- [X] T053 [US2] Implement LibraryStorage availability and list query methods in src/library/storage.rs
- [X] T054 [US2] Implement LibraryService scan reconciliation methods in src/library/service.rs
- [X] T055 [US2] Implement WatchEventBatch and event coalescing rules in src/library/watcher.rs
- [X] T056 [US2] Implement notify watcher startup and nonblocking event forwarding in src/library/watcher.rs
- [X] T057 [US2] Implement inaccessible-root and watcher-error recovery status in src/library/watcher.rs
- [X] T058 [US2] Add library synchronization exports in src/library/mod.rs

**Checkpoint**: User Story 2 is functional and testable with `cargo test --test library_scanner --test library_watcher --test library_storage`.

---

## Phase 5: User Story 3 - Browse Covers In A Responsive Grid (Priority: P3)

**Goal**: Provide a responsive cover grid backed by cached thumbnails and route
between library browsing and comic reading.

**Independent Test**: Load multiple comic records, resize the library view,
verify cover tiles wrap responsively, select a comic, and return from reading
to the library without losing collection state.

### Tests for User Story 3

- [X] T059 [P] [US3] Add thumbnail cache hit and miss tests in tests/thumbnail_cache.rs
- [X] T060 [P] [US3] Add thumbnail source invalidation tests in tests/thumbnail_cache.rs
- [X] T061 [P] [US3] Add thumbnail max 300px height tests in tests/thumbnail_cache.rs
- [X] T062 [P] [US3] Add recoverable corrupt or missing cover tests in tests/thumbnail_cache.rs
- [X] T063 [P] [US3] Add responsive grid column calculation tests in tests/app_routing.rs
- [X] T064 [P] [US3] Add AppState library-to-reading and reading-to-library transition tests in tests/app_routing.rs

### Implementation for User Story 3

- [X] T065 [US3] Implement thumbnail cache key and cache path resolution in src/library/thumbnails.rs
- [X] T066 [US3] Implement thumbnail cache validation and stale-entry detection in src/library/thumbnails.rs
- [X] T067 [US3] Implement cover extraction request creation using the first usable archive page in src/library/thumbnails.rs
- [X] T068 [US3] Implement thumbnail downsample and disk write helpers capped to 300px height in src/library/thumbnails.rs
- [X] T069 [US3] Implement recoverable thumbnail failure statuses in src/library/thumbnails.rs
- [X] T070 [US3] Implement AppState transitions and command methods in src/app/mod.rs
- [X] T071 [US3] Implement responsive library grid column calculation in src/app/ui.rs
- [X] T072 [US3] Implement library cover grid rendering with loading/error/ready tile states in src/app/ui.rs
- [X] T073 [US3] Implement comic tile selection routing to Reading state in src/app/ui.rs
- [X] T074 [US3] Implement reader-to-library navigation routing in src/app/ui.rs
- [X] T075 [US3] Wire top-level eframe update loop through AppState in src/main.rs
- [X] T076 [US3] Add app and thumbnail exports in src/lib.rs and src/library/mod.rs

**Checkpoint**: User Story 3 is functional and testable with `cargo test --test thumbnail_cache --test app_routing`.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validate the full feature, preserve architecture boundaries, and
ensure quickstart commands match the implementation.

- [X] T077 [P] Run cargo fmt and fix formatting issues across src/ and tests/
- [X] T078 [P] Run cargo clippy --all-targets -- -D warnings and address actionable warnings in src/ and tests/
- [X] T079 Run cargo test and confirm all feature and previous tests pass
- [X] T080 Run cargo check and confirm quickstart build validation passes
- [X] T081 Verify viewer modules do not import rusqlite, notify, scanner, watcher, or LibraryService directly in src/viewer/
- [X] T082 Verify app and viewer rendering paths do not call image decode, resize, archive read, SQLite, or thumbnail write APIs directly in src/app/ and src/viewer/
- [X] T083 Validate quickstart.md manual and focused test commands against implemented tests in specs/004-library-spread-ui/quickstart.md
- [X] T084 Update specs/004-library-spread-ui/quickstart.md if validation commands or manual smoke steps differ from implementation
- [X] T085 Review task completion and mark all finished tasks in specs/004-library-spread-ui/tasks.md

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies; can start immediately.
- **Foundational (Phase 2)**: Depends on Phase 1; blocks all user stories.
- **User Story 1 (Phase 3)**: Depends on Phase 2; suggested MVP.
- **User Story 2 (Phase 4)**: Depends on Phase 2 and can run independently of
  spread rendering after shared library models exist.
- **User Story 3 (Phase 5)**: Depends on Phase 2 and benefits from User Story 2
  scanner/listing behavior for a real collection, but thumbnail cache and app
  routing tests can be developed independently using view-model fixtures.
- **Polish (Phase 6)**: Depends on desired user stories being complete.

### User Story Dependencies

- **US1 Read With Optional Spreads (P1)**: Can start after foundations; no
  dependency on library scanning or grid routing.
- **US2 Keep Library Current Automatically (P2)**: Can start after foundations;
  independent of spread rendering.
- **US3 Browse Covers In A Responsive Grid (P3)**: Can start after foundations
  for app state, thumbnail cache, and grid layout; full end-to-end browsing
  integrates with US2 collection records.

### Within Each User Story

- Write tests before implementation tasks for that story.
- Implement pure models/decision helpers before service/UI integration.
- Keep scanner, watcher, thumbnail, and decode work outside egui render paths.
- Validate each story independently before moving to the next priority if
  working sequentially.

## Parallel Opportunities

- T011 through T015 can run in parallel after module naming decisions.
- T017, T018, T019, T022, T023, and T025 can run in parallel after setup, but
  tasks touching the same source file must be coordinated.
- T026 through T032 can be drafted in parallel for spread tests.
- T043 through T049 can be drafted in parallel for scanner/watcher/storage
  tests.
- T059 through T064 can be drafted in parallel for thumbnail and routing tests.
- After Phase 2, US1 and US2 can proceed in parallel because they primarily
  touch `src/viewer/` and `src/library/` respectively.

## Parallel Example: User Story 1

```bash
Task: "Add pre-stitched landscape spread renders-alone tests in tests/spread_viewer.rs"
Task: "Add last-page and missing-next-page fallback tests in tests/spread_viewer.rs"
Task: "Add side-by-side spread layout sizing tests in tests/viewer_layout.rs"
```

## Parallel Example: User Story 2

```bash
Task: "Add recursive supported archive discovery tests in tests/library_scanner.rs"
Task: "Add watcher event coalescing tests in tests/library_watcher.rs"
Task: "Add LibraryService availability update tests in tests/library_storage.rs"
```

## Parallel Example: User Story 3

```bash
Task: "Add thumbnail source invalidation tests in tests/thumbnail_cache.rs"
Task: "Add responsive grid column calculation tests in tests/app_routing.rs"
Task: "Add AppState library-to-reading and reading-to-library transition tests in tests/app_routing.rs"
```

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 setup.
2. Complete Phase 2 foundations.
3. Complete Phase 3 spread mode.
4. Stop and validate with `cargo test --test spread_viewer --test viewer_layout --test viewer_interaction`.

### Incremental Delivery

1. Finish Setup and Foundational phases.
2. Deliver US1 for optional spread reading.
3. Deliver US2 for library root synchronization.
4. Deliver US3 for cover grid browsing and app routing.
5. Run Phase 6 checks and quickstart validation.

### Parallel Team Strategy

1. Complete Setup and Foundational phases together.
2. Assign US1 to viewer/spread behavior, US2 to library scanner/watcher
   behavior, and US3 to thumbnail/app-shell behavior.
3. Keep each story independently testable with its focused test command.
4. Integrate through public exports in `src/viewer/mod.rs`,
   `src/library/mod.rs`, and `src/app/mod.rs`.

## Notes

- `[P]` tasks indicate safe parallel opportunities, but tasks editing the same
  file still need coordination.
- Viewer UI must consume prepared page and thumbnail textures; it must not
  perform archive reads, decoding, resizing, SQLite scans, or texture upload
  during scroll/drag/spread-toggle handling.
- Library scanning and watching must update collection records through
  LibraryService rather than direct UI storage access.
- Thumbnail files are cache artifacts and must not be treated as extracted
  source archive payloads.
