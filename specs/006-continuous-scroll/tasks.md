# Tasks: Continuous Vertical Scroll

**Input**: Design documents from `/specs/006-continuous-scroll/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Test tasks are included because the feature plan requires focused geometry, app routing, cache, and viewer interaction validation for continuous scrolling.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Every task includes an exact file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm current reader primitives and add reusable test fixtures.

- [x] T001 Review existing continuous mode, spread, viewer, app routing, and cache APIs in src/viewer/spread.rs, src/viewer/ui.rs, src/viewer/state.rs, src/app/ui.rs, src/app/mod.rs, and src/cache/page_cache.rs
- [x] T002 [P] Add multi-page mixed-size CBZ fixture helpers for continuous scroll tests in tests/app_routing.rs
- [x] T003 [P] Add quickstart implementation notes for continuous scroll validation in specs/006-continuous-scroll/quickstart.md

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add pure geometry and session state required by all continuous-scroll stories.

**CRITICAL**: No user story work can begin until this phase is complete.

- [x] T004 Define PageMeasurement, PageMeasurementSource, VirtualPageRect, VirtualCanvas, VisiblePageWindow, and ContinuousScrollState types in src/viewer/continuous.rs
- [x] T005 Export the new continuous scroll module and public types from src/viewer/mod.rs
- [x] T006 Add ContinuousScrollState ownership to ReadingSession in src/app/mod.rs
- [x] T007 Initialize and reset ContinuousScrollState when creating or replacing ReadingSession in src/app/mod.rs
- [x] T008 Implement placeholder dimension calculation using first known page ratio and default portrait fallback in src/viewer/continuous.rs
- [x] T009 Implement virtual canvas page rectangle construction and total height calculation in src/viewer/continuous.rs
- [x] T010 Implement viewport intersection and one-page overdraw window selection in src/viewer/continuous.rs
- [x] T011 [P] Add unit tests for placeholder dimension fallback and first-known-page ratio behavior in tests/viewer_layout.rs
- [x] T012 [P] Add unit tests for virtual canvas total height, ordered page rectangles, and resize recalculation in tests/viewer_layout.rs
- [x] T013 [P] Add unit tests for visible viewport intersection and one-page overdraw selection in tests/viewer_layout.rs

**Checkpoint**: Continuous layout geometry is testable without rendering textures or loading archives.

---

## Phase 3: User Story 1 - Read Continuously (Priority: P1) MVP

**Goal**: Let readers switch into a continuous vertical mode and scroll through pages in one uninterrupted column.

**Independent Test**: Open a multi-page comic, enable continuous vertical layout, and scroll from page 1 into later pages without using next/previous navigation.

### Tests for User Story 1

- [x] T014 [P] [US1] Add viewer state test for toggling ReadingLayoutMode::ContinuousVertical and resetting incompatible spread state in tests/viewer_interaction.rs
- [x] T015 [P] [US1] Add app routing test that continuous mode can render a multi-page virtual canvas from cached page entries in tests/app_routing.rs
- [x] T016 [P] [US1] Add mode-switch test that returning from continuous mode lands on the nearest visible page in tests/viewer_interaction.rs

### Implementation for User Story 1

- [x] T017 [US1] Add continuous mode toolbar toggle and pending view command handling in src/app/ui.rs
- [x] T018 [US1] Extend ViewCommand and ViewerState layout mode transitions for continuous vertical mode in src/viewer/state.rs
- [x] T019 [US1] Route viewer rendering to paged or continuous rendering based on ViewerState.layout_mode in src/viewer/ui.rs
- [x] T020 [US1] Implement continuous ScrollArea virtual canvas allocation and page placeholder painting in src/viewer/ui.rs
- [x] T021 [US1] Paint cached continuous pages at their virtual canvas rectangles in src/viewer/ui.rs
- [x] T022 [US1] Disable side-by-side spread composition while continuous vertical layout is active in src/viewer/ui.rs
- [x] T023 [US1] Update app route handling so switching back to paged mode sets ReadingSession.current_page_index to the nearest visible page in src/app/ui.rs

**Checkpoint**: User Story 1 is complete when continuous mode displays an uninterrupted vertical page flow and mode switching preserves a sensible page.

---

## Phase 4: User Story 2 - Keep Scrolling Responsive (Priority: P2)

**Goal**: Prepare only visible and near-visible pages during continuous scrolling so large comics remain responsive.

**Independent Test**: Scroll rapidly through a large comic and verify only pages intersecting the viewport plus one page above and below are requested.

### Tests for User Story 2

- [x] T024 [P] [US2] Add app routing test that continuous visible-window dispatch requests only visible pages plus one overdraw page on each side in tests/app_routing.rs
- [x] T025 [P] [US2] Add app routing test that distant pages outside the continuous visible window are not submitted when scrolling in tests/app_routing.rs
- [x] T026 [P] [US2] Add cancellation test for stale continuous-window page requests after a rapid scroll jump in tests/app_routing.rs
- [x] T027 [P] [US2] Add cache bound test proving continuous dispatch does not grow PageTextureCache beyond configured capacity in tests/page_cache.rs

### Implementation for User Story 2

- [x] T028 [US2] Add continuous visible-window dispatch helper that checks cached, queued, and in-flight pages before submission in src/app/ui.rs
- [x] T029 [US2] Reuse WorkerPool and DecodePurpose::Prefetch for missing near-visible continuous pages in src/app/ui.rs
- [x] T030 [US2] Extend PrefetchRuntime stale cancellation to accept continuous visible-window candidates in src/app/mod.rs
- [x] T031 [US2] Reconcile continuous prefetch results into PageTextureCache and ContinuousScrollState measurements in src/app/ui.rs
- [x] T032 [US2] Request egui repaint while continuous near-visible decode work is in flight without blocking the render loop in src/app/ui.rs
- [x] T033 [US2] Render loading placeholders for near-visible pages that are queued or in flight in src/viewer/ui.rs
- [x] T034 [US2] Ensure continuous rendering uses cache checks only and performs no archive reads or image decodes from src/viewer/ui.rs

**Checkpoint**: User Story 2 is complete when continuous scrolling prepares a bounded near-visible page window and remains responsive during rapid scroll.

---

## Phase 5: User Story 3 - Stable Layout While Sizes Become Known (Priority: P3)

**Goal**: Keep the continuous document stable while real page dimensions replace placeholders asynchronously.

**Independent Test**: Open a comic before later pages are measured, confirm placeholders reserve space, then confirm actual dimensions update without jumping away from the reading location.

### Tests for User Story 3

- [x] T035 [P] [US3] Add measurement transition tests for Unknown to Placeholder, Placeholder to Actual, and Placeholder to FailedPlaceholder in tests/viewer_layout.rs
- [x] T036 [P] [US3] Add scroll-anchor preservation tests for actual measurement updates before the current viewport in tests/viewer_interaction.rs
- [x] T037 [P] [US3] Add corrupt near-visible page placeholder test for continuous mode in tests/app_routing.rs
- [x] T038 [P] [US3] Add resize recalculation test that keeps continuous page rectangles positive and ordered in tests/viewer_layout.rs

### Implementation for User Story 3

- [x] T039 [US3] Record actual page measurements from cached direct loads and successful prefetch results in src/app/ui.rs
- [x] T040 [US3] Preserve scroll anchor by nearest visible page and intra-page offset when measurements change in src/viewer/continuous.rs
- [x] T041 [US3] Apply scroll-anchor restoration after continuous canvas recalculation in src/viewer/ui.rs
- [x] T042 [US3] Record recoverable failed page measurements and messages for corrupt continuous pages in src/app/ui.rs
- [x] T043 [US3] Paint failed continuous page placeholders without blocking adjacent page rendering in src/viewer/ui.rs
- [x] T044 [US3] Update window resize handling so placeholder and actual continuous rectangles recompute from current viewport width in src/viewer/ui.rs

**Checkpoint**: User Story 3 is complete when placeholder geometry is stable, actual measurements update smoothly, and corrupt pages remain recoverable.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validate performance, documentation, and behavior across all stories.

- [x] T045 [P] Update continuous scroll quickstart with observed implementation behavior and known limitations in specs/006-continuous-scroll/quickstart.md
- [x] T046 [P] Add concise comments for non-obvious scroll anchoring and visible-window cancellation logic in src/viewer/continuous.rs and src/app/mod.rs
- [x] T047 Run focused validation commands from specs/006-continuous-scroll/quickstart.md
- [x] T048 Run full validation commands from specs/006-continuous-scroll/quickstart.md
- [ ] T049 Manually validate continuous scrolling, rapid scroll responsiveness, mode switching, window resizing, and corrupt page recovery using specs/006-continuous-scroll/quickstart.md

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies.
- **Foundational (Phase 2)**: Depends on Setup completion and blocks all user stories.
- **User Story 1 (Phase 3)**: Depends on Foundational phase.
- **User Story 2 (Phase 4)**: Depends on Foundational phase and can be developed after US1 rendering route exists.
- **User Story 3 (Phase 5)**: Depends on Foundational phase and benefits from US1/US2 integration.
- **Polish (Phase 6)**: Depends on desired user stories being complete.

### User Story Dependencies

- **User Story 1 (P1)**: MVP. Enables continuous vertical mode and basic scroll rendering.
- **User Story 2 (P2)**: Builds on continuous visible-window geometry to keep large-comic scrolling bounded and responsive.
- **User Story 3 (P3)**: Builds on measurement and rendering paths to smooth placeholder-to-actual transitions and failure placeholders.

### Parallel Opportunities

- T002 and T003 can run in parallel after T001.
- T011, T012, and T013 can run in parallel after T004-T010 interfaces are agreed.
- US1 tests T014, T015, and T016 can run in parallel.
- US2 tests T024, T025, T026, and T027 can run in parallel.
- US3 tests T035, T036, T037, and T038 can run in parallel.
- T045 and T046 can run in parallel after implementation stabilizes.

---

## Parallel Example: User Story 1

```bash
Task: "Add viewer state test for toggling ReadingLayoutMode::ContinuousVertical and resetting incompatible spread state in tests/viewer_interaction.rs"
Task: "Add app routing test that continuous mode can render a multi-page virtual canvas from cached page entries in tests/app_routing.rs"
Task: "Add mode-switch test that returning from continuous mode lands on the nearest visible page in tests/viewer_interaction.rs"
```

## Parallel Example: User Story 2

```bash
Task: "Add app routing test that continuous visible-window dispatch requests only visible pages plus one overdraw page on each side in tests/app_routing.rs"
Task: "Add app routing test that distant pages outside the continuous visible window are not submitted when scrolling in tests/app_routing.rs"
Task: "Add cancellation test for stale continuous-window page requests after a rapid scroll jump in tests/app_routing.rs"
```

## Parallel Example: User Story 3

```bash
Task: "Add measurement transition tests for Unknown to Placeholder, Placeholder to Actual, and Placeholder to FailedPlaceholder in tests/viewer_layout.rs"
Task: "Add scroll-anchor preservation tests for actual measurement updates before the current viewport in tests/viewer_interaction.rs"
Task: "Add corrupt near-visible page placeholder test for continuous mode in tests/app_routing.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 setup.
2. Complete Phase 2 foundational continuous geometry and state.
3. Complete Phase 3 User Story 1.
4. Validate continuous mode can scroll through multiple cached or placeholder pages without next/previous navigation.
5. Stop and confirm paged mode still works before adding viewport-triggered background preparation.

### Incremental Delivery

1. Add pure geometry and session state.
2. Deliver continuous mode rendering with placeholder/cached pages.
3. Add near-visible dispatch, cancellation, and bounded cache behavior.
4. Add placeholder-to-actual measurement stability and corrupt-page placeholders.
5. Run focused and full validation, then manual quickstart.

### Risk Controls

- Keep archive reads, image decoding, PDF rendering, and resizing out of the render loop.
- Keep continuous measurements lightweight and separate from texture residency.
- Reuse existing worker, cancellation, and texture-cache paths instead of adding a second page-loading system.
- Keep spread behavior paged-only while continuous mode is active.
