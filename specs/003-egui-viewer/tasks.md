# Tasks: egui Viewer Implementation

**Input**: Design documents from `/specs/003-egui-viewer/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/egui-viewer.md, quickstart.md

**Prerequisite Note**: `.specify/scripts/bash/check-prerequisites.sh --json`
reported `ERROR: Not on a feature branch. Current branch: HEAD` because the Git
repository has no current commit. Tasks are generated from the active feature
directory `specs/003-egui-viewer`.

**Tests**: Included because the feature specification defines independent tests
for each user story and quickstart validation expects focused `cargo test`
coverage for layout, zoom/pan, and page-change reset behavior.

**Organization**: Tasks are grouped by user story so each story can be
implemented and tested independently after shared foundations are complete.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel because it touches different files or depends
  only on completed prerequisites
- **[Story]**: User story label, required only for user story phases
- Every task includes an exact repository-relative file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create viewer modules and test files for layout, interaction, and
rendering integration work.

- [X] T001 Create viewer layout module placeholder in src/viewer/layout.rs
- [X] T002 Create viewer state module placeholder in src/viewer/state.rs
- [X] T003 Create viewer UI module placeholder in src/viewer/ui.rs
- [X] T004 Wire layout, state, and ui modules through src/viewer/mod.rs
- [X] T005 [P] Create placeholder viewer layout integration test module in tests/viewer_layout.rs
- [X] T006 [P] Create placeholder viewer interaction integration test module in tests/viewer_interaction.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define shared viewer state, geometry types, and page-status
contracts required by all stories.

**Critical**: No user story implementation should begin until this phase is
complete.

- [X] T007 Define PageId, Size2, Point2, and ViewMode types in src/viewer/layout.rs
- [X] T008 Define ZoomPanState with min/max zoom, pan offsets, and reset generation in src/viewer/state.rs
- [X] T009 Define PageStatus<T> and ViewerState<T> in src/viewer/state.rs
- [X] T010 Define ViewerChrome state for minimal loading/error/status affordances in src/viewer/state.rs
- [X] T011 Wire shared viewer types through src/viewer/mod.rs

**Checkpoint**: Shared viewer state and layout contracts are ready for user
story implementation.

---

## Phase 3: User Story 1 - Read a Single Page Comfortably (Priority: P1) MVP

**Goal**: Display a prepared page in the primary reading area with aspect-ratio
preserving fit/fill behavior that responds to viewport resize.

**Independent Test**: Provide representative page and viewport dimensions, then
confirm the computed display size remains proportional, visible in fit mode,
and correctly scaled in fill mode.

### Tests for User Story 1

- [X] T012 [P] [US1] Add portrait, landscape, square, tall, and spread fit-mode tests in tests/viewer_layout.rs
- [X] T013 [P] [US1] Add fill-mode aspect ratio preservation tests in tests/viewer_layout.rs
- [X] T014 [P] [US1] Add zero or invalid dimension safe layout tests in tests/viewer_layout.rs
- [X] T015 [P] [US1] Add ViewerState ready/loading/failed page status tests in tests/viewer_layout.rs

### Implementation for User Story 1

- [X] T016 [US1] Implement page_display_size fit and fill calculations in src/viewer/layout.rs
- [X] T017 [US1] Implement PageStatus<T> helpers for empty, loading, ready, and failed states in src/viewer/state.rs
- [X] T018 [US1] Implement ViewerState<T> page status update methods in src/viewer/state.rs
- [X] T019 [US1] Implement egui CentralPanel single-page rendering entry point in src/viewer/ui.rs
- [X] T020 [US1] Implement missing/loading/error recoverable presentation in src/viewer/ui.rs

**Checkpoint**: User Story 1 is functional and testable with `cargo test --test viewer_layout`.

---

## Phase 4: User Story 2 - Zoom and Pan Around Page Details (Priority: P2)

**Goal**: Support smooth bounded zoom and pan behavior while keeping interaction
state stable and cheap to update.

**Independent Test**: Apply scroll and drag deltas to a page/viewport fixture,
then confirm zoom clamps correctly and pan remains within useful bounds.

### Tests for User Story 2

- [X] T021 [P] [US2] Add scroll zoom increases and decreases scale within bounds tests in tests/viewer_interaction.rs
- [X] T022 [P] [US2] Add zoom clamp minimum and maximum tests in tests/viewer_interaction.rs
- [X] T023 [P] [US2] Add drag pan only when zoomed page exceeds viewport tests in tests/viewer_interaction.rs
- [X] T024 [P] [US2] Add pan clamping to useful page bounds tests in tests/viewer_interaction.rs
- [X] T025 [P] [US2] Add repeated zoom and pan stability test in tests/viewer_interaction.rs

### Implementation for User Story 2

- [X] T026 [US2] Implement ZoomPanState::apply_scroll_zoom in src/viewer/state.rs
- [X] T027 [US2] Implement pan_bounds and clamp_pan helpers in src/viewer/layout.rs
- [X] T028 [US2] Implement ZoomPanState::apply_drag_pan using layout bounds in src/viewer/state.rs
- [X] T029 [US2] Integrate egui ScrollArea, scroll-wheel zoom, and drag pan handling in src/viewer/ui.rs

**Checkpoint**: User Story 2 is functional and testable with `cargo test --test viewer_interaction`.

---

## Phase 5: User Story 3 - Preserve Modern Reading Flow Across Page Changes (Priority: P3)

**Goal**: Reset zoom and pan on page change and keep new page layout based on
the new page dimensions.

**Independent Test**: Zoom and pan one page, switch page identity, and confirm
zoom/pan reset while the new page computes its own fitted size.

### Tests for User Story 3

- [X] T030 [P] [US3] Add page identity change resets zoom and pan test in tests/viewer_interaction.rs
- [X] T031 [P] [US3] Add same page identity preserves zoom and pan test in tests/viewer_interaction.rs
- [X] T032 [P] [US3] Add different page aspect ratio recomputes fit size test in tests/viewer_layout.rs
- [X] T033 [P] [US3] Add active input stale state does not affect new page test in tests/viewer_interaction.rs

### Implementation for User Story 3

- [X] T034 [US3] Implement ZoomPanState::reset_for_page and reset generation tracking in src/viewer/state.rs
- [X] T035 [US3] Implement ViewerState<T>::set_current_page reset behavior in src/viewer/state.rs
- [X] T036 [US3] Apply reset-on-page-change behavior in src/viewer/ui.rs
- [X] T037 [US3] Add minimal non-obstructive status chrome behavior in src/viewer/ui.rs

**Checkpoint**: User Story 3 is functional and testable with `cargo test --test viewer_interaction`.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validate full viewer behavior and preserve architecture boundaries.

- [X] T038 [P] Run cargo fmt and fix formatting issues across src/ and tests/
- [X] T039 [P] Run cargo clippy --all-targets -- -D warnings and address actionable warnings in src/ and tests/
- [X] T040 Run cargo test and confirm viewer plus previous feature tests pass
- [X] T041 Verify viewer modules do not import LibraryService or VFS archive readers directly in src/viewer/
- [X] T042 Verify scroll and drag handling in src/viewer/ui.rs does not perform image decoding, resizing, archive reads, SQLite scans, or texture upload
- [X] T043 Validate quickstart.md instructions against implemented commands in specs/003-egui-viewer/quickstart.md
- [X] T044 Update specs/003-egui-viewer/quickstart.md if validation commands or viewer scope differ from implementation

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies; can start immediately.
- **Foundational (Phase 2)**: Depends on Phase 1; blocks all user stories.
- **User Story 1 (Phase 3)**: Depends on Phase 2; suggested MVP.
- **User Story 2 (Phase 4)**: Depends on Phase 2 and can run after or alongside US1 using pure interaction fixtures, but UI integration benefits from US1 rendering structure.
- **User Story 3 (Phase 5)**: Depends on Phase 2 and reset behavior integrates with US1/US2 state.
- **Polish (Phase 6)**: Depends on desired user stories being complete.

### User Story Dependencies

- **US1 (P1)**: No dependency on US2 or US3 after foundations.
- **US2 (P2)**: Uses shared layout/state types and can be tested independently of full rendering.
- **US3 (P3)**: Uses shared state and can be tested independently, then integrated into UI page-change flow.

### Within Each User Story

- Write tests before implementation tasks for that story.
- Keep pure layout and interaction math separate from egui rendering where possible.
- Keep ViewerState free of library persistence and archive byte access.
- Validate each story independently before moving to the next priority if working sequentially.

## Parallel Opportunities

- T005 and T006 can run in parallel after setup module decisions.
- T012 through T015 can be drafted in parallel with clear ownership of separate test cases in tests/viewer_layout.rs.
- T021 through T025 can be drafted in parallel with clear ownership of separate test cases in tests/viewer_interaction.rs.
- T030 through T033 can be drafted in parallel with clear ownership of separate reset/layout test cases.
- After Phase 2, US1 layout work and US2 pure interaction work can proceed in parallel if edits to src/viewer/state.rs are coordinated.

## Parallel Example: User Story 1

```bash
Task: "Add portrait, landscape, square, tall, and spread fit-mode tests in tests/viewer_layout.rs"
Task: "Add fill-mode aspect ratio preservation tests in tests/viewer_layout.rs"
Task: "Add ViewerState ready/loading/failed page status tests in tests/viewer_layout.rs"
```

## Parallel Example: User Story 2

```bash
Task: "Add scroll zoom increases and decreases scale within bounds tests in tests/viewer_interaction.rs"
Task: "Add drag pan only when zoomed page exceeds viewport tests in tests/viewer_interaction.rs"
Task: "Add pan clamping to useful page bounds tests in tests/viewer_interaction.rs"
```

## Parallel Example: User Story 3

```bash
Task: "Add page identity change resets zoom and pan test in tests/viewer_interaction.rs"
Task: "Add same page identity preserves zoom and pan test in tests/viewer_interaction.rs"
Task: "Add different page aspect ratio recomputes fit size test in tests/viewer_layout.rs"
```

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 setup.
2. Complete Phase 2 foundations.
3. Complete Phase 3 layout and single-page rendering.
4. Stop and validate with `cargo test --test viewer_layout`.

### Incremental Delivery

1. Finish Setup and Foundational phases.
2. Deliver US1 for aspect-correct single-page display.
3. Deliver US2 for bounded zoom and pan.
4. Deliver US3 for page-change reset and reading flow polish.
5. Run Phase 6 checks and quickstart validation.

### Parallel Team Strategy

1. Complete Setup and Foundational phases together.
2. Assign US1 to layout/rendering, US2 to zoom/pan interaction, and US3 to page reset/chrome behavior.
3. Keep each story independently testable using the story-specific test command.
4. Integrate through public exports in src/viewer/mod.rs.

## Notes

- `[P]` tasks indicate safe parallel opportunities, but tasks editing the same
  file still need coordination.
- Viewer UI must consume prepared textures from the async image pipeline/cache.
- Viewer modules must not perform archive reads, decoding, resizing, SQLite
  scans, or texture upload during scroll/drag handling.
- Default mode keeps the whole page visible; fill mode may crop only when
  explicitly selected.
