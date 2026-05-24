# Tasks: Library Metadata Surfacing

**Input**: Design documents from `specs/008-library-metadata-surfacing/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/library-metadata-ui.md, quickstart.md

**Tests**: Included because the specification defines independent tests and quickstart automated validation for storage, app routing, and metadata behavior.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel with other marked tasks in the same phase because it touches different files or only adds independent tests
- **[Story]**: Maps task to a user story from `spec.md`
- Every task includes an exact file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare focused test fixtures and confirm the current feature surface before model changes.

- [x] T001 [P] Add helper builders for metadata-rich comics and grid items in `tests/app_routing.rs`
- [x] T002 [P] Add storage fixture helpers for comics with linked metadata rows in `tests/library_storage.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define shared metadata/filter types that all user stories rely on.

**CRITICAL**: No user story work should begin until these model and state foundations are in place.

- [x] T003 Add `ComicMetadataDisplay`, `LibraryGroupKind`, `LibraryGroup`, and `ActiveLibraryFilter` models in `src/library/models.rs`
- [x] T004 Extend `LibraryGridItem` with `subtitle`, `series`, `series_key`, `folder_label`, and `folder_key` fields in `src/library/models.rs`
- [x] T005 Add `active_filter` state to `LibraryViewState` in `src/app/mod.rs`
- [x] T006 Update existing `LibraryGridItem` construction sites for the new fields in `tests/app_routing.rs`

**Checkpoint**: Shared model/state compiles and user story work can proceed.

---

## Phase 3: User Story 1 - See Metadata In Library Cards (Priority: P1) MVP

**Goal**: Library thumbnail cards and list rows show a concise metadata subtitle when a comic has useful ComicInfo metadata.

**Independent Test**: Scan or seed comics with known metadata, load the library, and confirm matching items show issue/creator subtitles while comics with no metadata remain visible without placeholders.

### Tests for User Story 1

- [x] T007 [P] [US1] Add a LEFT JOIN regression test for comics with and without metadata in `tests/library_storage.rs`
- [x] T008 [P] [US1] Add subtitle formatting tests for issue/writer, issue-only, writer-only, penciller fallback, and no-metadata cases in `tests/app_routing.rs`

### Implementation for User Story 1

- [x] T009 [US1] Add a metadata-aware comic row query with `LEFT JOIN metadata` in `src/library/storage.rs`
- [x] T010 [US1] Expose metadata-aware library row loading through `LibraryService::library_grid_items` in `src/library/service.rs`
- [x] T011 [US1] Implement subtitle formatting helpers that omit blank fields and duplicate separators in `src/library/service.rs`
- [x] T012 [US1] Populate `LibraryGridItem` subtitle and metadata fields from joined rows in `src/library/service.rs`
- [x] T013 [US1] Render optional subtitles in thumbnail tiles and list rows without layout overlap in `src/app/ui.rs`

**Checkpoint**: User Story 1 is complete when metadata-rich library items display subtitles and metadata-free items keep the existing title-only behavior.

---

## Phase 4: User Story 2 - Filter Library By Series Or Folder (Priority: P2)

**Goal**: Users can narrow the library by unique series names or containing folders and clear the filter to restore all items.

**Independent Test**: Seed a mixed library, select one series or folder group, verify only matching comics render, then clear the selection and verify all items return.

### Tests for User Story 2

- [x] T014 [P] [US2] Add group derivation tests for normalized series names and item counts in `tests/app_routing.rs`
- [x] T015 [P] [US2] Add folder fallback filtering tests for comics with no series metadata in `tests/app_routing.rs`
- [x] T016 [P] [US2] Add active-filter clearing test when a library refresh removes the selected group in `tests/app_routing.rs`

### Implementation for User Story 2

- [x] T017 [US2] Implement library group derivation for series and folders in `src/app/mod.rs`
- [x] T018 [US2] Implement in-memory visible-item filtering by `ActiveLibraryFilter` in `src/app/mod.rs`
- [x] T019 [US2] Clear invalid active filters after library hydration or scan refresh in `src/app/mod.rs`
- [x] T020 [US2] Add series/folder filter controls with an all-items option in `render_library_root_controls` in `src/app/ui.rs`
- [x] T021 [US2] Pass filtered visible items to `render_library_grid` and `render_library_list` in `route_app_update` in `src/app/ui.rs`

**Checkpoint**: User Story 2 is complete when filtering works in both library view modes and clearing the filter restores the full loaded library.

---

## Phase 5: User Story 3 - Preserve Library Interactions While Filtered (Priority: P3)

**Goal**: Existing library behavior remains intact while a filter is active.

**Independent Test**: Apply a filter and verify thumbnails still load, list/thumbnail switching preserves the filter, visible comics open normally, and unavailable comics remain blocked.

### Tests for User Story 3

- [x] T022 [P] [US3] Add regression tests for opening a visible filtered comic and blocking an unavailable filtered comic in `tests/app_routing.rs`
- [x] T023 [P] [US3] Add regression tests that thumbnail scheduling still considers filtered visible items and existing thumbnail textures remain usable in `tests/app_routing.rs`
- [x] T024 [P] [US3] Add regression test that switching `LibraryViewMode` preserves `active_filter` in `tests/app_routing.rs`

### Implementation for User Story 3

- [x] T025 [US3] Ensure thumbnail polling and scheduling continue to work with filtered library rendering in `src/app/ui.rs`
- [x] T026 [US3] Ensure selecting a filtered item opens the reader using the existing full library item lookup path in `src/app/ui.rs`
- [x] T027 [US3] Ensure unavailable item styling and open blocking remain unchanged for filtered items in `src/app/ui.rs`

**Checkpoint**: User Story 3 is complete when filtered browsing does not regress thumbnails, open behavior, unavailable markers, or view mode switching.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validate performance expectations, cleanup, and documentation.

- [x] T028 [P] Update `specs/008-library-metadata-surfacing/quickstart.md` if implementation changes the manual validation steps
- [x] T029 Run `cargo test --test library_storage --test library_scanner --test app_routing --test metadata_parser` and fix failures in `src/library/` or `src/app/`
- [x] T030 Run `cargo clippy --all-targets -- -D warnings` and fix warnings in `src/library/`, `src/app/`, or tests
- [ ] T031 Manually validate library subtitle rendering and series/folder filtering against a local scanned library per `specs/008-library-metadata-surfacing/quickstart.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies; can start immediately.
- **Foundational (Phase 2)**: Depends on Setup; blocks all user stories because model changes affect constructors and UI rendering.
- **User Story 1 (Phase 3)**: Depends on Foundational; delivers the MVP metadata subtitle value.
- **User Story 2 (Phase 4)**: Depends on Foundational; can start after metadata/group fields exist, but benefits from US1-populated series/folder fields.
- **User Story 3 (Phase 5)**: Depends on US2 because it validates filtered-view preservation.
- **Polish (Phase 6)**: Depends on desired user stories being complete.

### User Story Dependencies

- **US1 (P1)**: Can start after Foundational; no dependency on US2 or US3.
- **US2 (P2)**: Can start after Foundational; requires `LibraryGridItem` group fields from T004.
- **US3 (P3)**: Starts after US2 filtering exists.

### Within Each User Story

- Test tasks should be written before implementation tasks for the same story.
- Storage/service changes precede UI rendering changes.
- App state filtering helpers precede UI filter controls.
- Regression tasks in US3 validate that filtered rendering does not bypass existing library behavior.

### Parallel Opportunities

- T001 and T002 can run in parallel.
- T007 and T008 can run in parallel.
- T014, T015, and T016 can run in parallel.
- T022, T023, and T024 can run in parallel.
- T028 can run in parallel with final validation if manual steps do not change.

---

## Parallel Example: User Story 1

```text
Task: "Add a LEFT JOIN regression test for comics with and without metadata in tests/library_storage.rs"
Task: "Add subtitle formatting tests for issue/writer, issue-only, writer-only, penciller fallback, and no-metadata cases in tests/app_routing.rs"
```

---

## Parallel Example: User Story 2

```text
Task: "Add group derivation tests for normalized series names and item counts in tests/app_routing.rs"
Task: "Add folder fallback filtering tests for comics with no series metadata in tests/app_routing.rs"
Task: "Add active-filter clearing test when a library refresh removes the selected group in tests/app_routing.rs"
```

---

## Parallel Example: User Story 3

```text
Task: "Add regression tests for opening a visible filtered comic and blocking an unavailable filtered comic in tests/app_routing.rs"
Task: "Add regression tests that thumbnail scheduling still considers filtered visible items and existing thumbnail textures remain usable in tests/app_routing.rs"
Task: "Add regression test that switching LibraryViewMode preserves active_filter in tests/app_routing.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 and Phase 2.
2. Complete Phase 3 for metadata subtitles.
3. Stop and validate with `cargo test --test library_storage --test app_routing --test metadata_parser`.
4. Demo metadata subtitles in thumbnail and list views.

### Incremental Delivery

1. Add metadata fields and subtitles.
2. Add series/folder grouping and filtering.
3. Add filtered-view regression hardening.
4. Run quickstart validation and clippy.

### Parallel Team Strategy

After foundational model changes are in place:
- One developer can work on storage/service subtitle behavior.
- One developer can work on app-state filter helpers.
- One developer can work on UI rendering and regression tests once helper signatures stabilize.

## Notes

- Keep SQLite access inside `LibraryService`/`LibraryStorage`; do not query from egui UI code.
- Keep filtering in memory over hydrated `LibraryGridItem` values.
- Do not modify ViewerState, VFS page loading, decode workers, or page texture cache behavior for this feature.
- Every implementation task should preserve existing thumbnail loading and unavailable-comic behavior.
