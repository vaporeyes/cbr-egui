# Tasks: Prefetch & VRAM Cache Integration

**Input**: Design documents from `/specs/005-prefetch-vram-cache/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Test tasks are included because the feature plan requires focused validation for prefetch dispatch, cancellation, cache reconciliation, and navigation cache hits.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Every task includes an exact file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm existing primitives and create reusable test fixtures before story work.

- [x] T001 Review existing prefetch candidate, decode worker, and texture cache APIs in src/viewer/prefetch.rs, src/decode/worker.rs, src/decode/pipeline.rs, and src/cache/page_cache.rs
- [x] T002 [P] Add shared multi-page CBZ fixture helpers for reader prefetch tests in tests/app_routing.rs
- [x] T003 [P] Add task-specific quickstart notes for manual cache/prefetch validation in specs/005-prefetch-vram-cache/quickstart.md

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add session-scoped state and helper contracts required by all stories.

**CRITICAL**: No user story work can begin until this phase is complete.

- [x] T004 Define PrefetchRuntime and InFlightPrefetch session types in src/app/mod.rs
- [x] T005 Add PageTextureCache<egui::TextureHandle> ownership to ReadingSession in src/app/mod.rs
- [x] T006 Add WorkerPool ownership and initialization for active reader sessions in src/app/mod.rs
- [x] T007 Add request ID and generation allocation helpers for prefetch submissions in src/app/mod.rs
- [x] T008 [P] Add helper to collect cached page indices from PageTextureCache in src/cache/page_cache.rs
- [x] T009 [P] Add tests for ReadingSession prefetch runtime defaults and bounded cache initialization in tests/app_routing.rs
- [x] T010 Add cleanup behavior that cancels outstanding prefetch work when returning to the library in src/app/mod.rs

**Checkpoint**: ReadingSession can own prefetch metadata, worker pool, and bounded cache without dispatching work yet.

---

## Phase 3: User Story 1 - Nearby Pages Open Instantly (Priority: P1) MVP

**Goal**: Dispatch nearby-page background preparation when the active page is ready or changes.

**Independent Test**: Open a readable comic on page 10 and verify pages 11, 12, and 9 are submitted for preparation when not already cached, queued, or in flight.

### Tests for User Story 1

- [x] T011 [P] [US1] Add dispatch test for next, following, and previous page submissions in tests/app_routing.rs
- [x] T012 [P] [US1] Add dispatch exclusion test for cached, queued, in-flight, and out-of-range pages in tests/prefetch_scheduler.rs

### Implementation for User Story 1

- [x] T013 [US1] Add VFS page-byte reader helper usable by direct loads and prefetch dispatch in src/app/ui.rs
- [x] T014 [US1] Implement build_prefetch_state_from_session helper using cache, queued, and in-flight sets in src/app/mod.rs
- [x] T015 [US1] Implement dispatch_prefetch_for_current_page using prefetch_candidates and DecodePurpose::Prefetch in src/app/ui.rs
- [x] T016 [US1] Store CancellationToken, request ID, page index, and generation for every submitted prefetch in src/app/mod.rs
- [x] T017 [US1] Call dispatch_prefetch_for_current_page after a page becomes ready and after page navigation in src/app/ui.rs
- [x] T018 [US1] Request egui repaint while prefetch work is in flight without blocking the render loop in src/app/ui.rs

**Checkpoint**: User Story 1 is complete when nearby page decode requests are dispatched once per useful candidate and the visible page remains responsive.

---

## Phase 4: User Story 2 - Background Results Become Displayable Cache Entries (Priority: P2)

**Goal**: Poll completed background decodes, upload successful pages as textures, and insert them into the bounded cache for reuse.

**Independent Test**: Trigger prefetch, poll the worker result, and verify the completed page becomes a cache hit used by navigation.

### Tests for User Story 2

- [x] T019 [P] [US2] Add reconciliation test that inserts successful prefetch results into the page texture cache in tests/app_routing.rs
- [x] T020 [P] [US2] Add navigation cache-hit test that avoids direct page reload when a page is already cached in tests/app_routing.rs
- [x] T021 [P] [US2] Add recoverable failed-prefetch result test that removes in-flight tracking without blocking visible page state in tests/app_routing.rs

### Implementation for User Story 2

- [x] T022 [US2] Implement non-blocking poll_decode_results loop for active reading sessions in src/app/ui.rs
- [x] T023 [US2] Validate worker results against in-flight request ID and generation before cache insertion in src/app/mod.rs
- [x] T024 [US2] Convert successful ColorImage results to TextureHandle and insert Display Cache Entry data in src/app/ui.rs
- [x] T025 [US2] Preserve pixel dimensions alongside cached textures for future ViewerState ready transitions in src/app/mod.rs
- [x] T026 [US2] Update load_reader_page to check PageTextureCache before direct archive read/decode in src/app/ui.rs
- [x] T027 [US2] Handle failed prefetch results by recording recoverable failure metadata and removing in-flight entries in src/app/mod.rs
- [x] T028 [US2] Ensure PageTextureCache eviction drops old TextureHandle entries during prefetch insertion in src/cache/page_cache.rs

**Checkpoint**: User Story 2 is complete when successful prefetches become reusable cached display entries and failed prefetches remain non-intrusive.

---

## Phase 5: User Story 3 - Stale Work Is Cancelled During Fast Navigation (Priority: P3)

**Goal**: Cancel obsolete in-flight requests when the reader jumps away from their nearby-page window and ignore late stale results.

**Independent Test**: Start prefetches near page 2, jump to page 50, and verify old requests are cancelled, removed from active tracking, and unable to overwrite useful cached content.

### Tests for User Story 3

- [x] T029 [P] [US3] Add cancellation test for stale in-flight requests after a large page jump in tests/app_routing.rs
- [x] T030 [P] [US3] Add late cancelled-result test that verifies stale results do not insert into PageTextureCache in tests/app_routing.rs
- [x] T031 [P] [US3] Add session teardown cancellation test for return_to_library in tests/app_routing.rs

### Implementation for User Story 3

- [x] T032 [US3] Implement stale prefetch detection for active page changes in src/app/mod.rs
- [x] T033 [US3] Invoke CancellationToken::cancel and remove obsolete in-flight entries before dispatching new candidates in src/app/ui.rs
- [x] T034 [US3] Ignore cancelled or unknown worker results during reconciliation in src/app/ui.rs
- [x] T035 [US3] Increment generation on page jumps and use generation checks to reject stale results in src/app/mod.rs
- [x] T036 [US3] Cancel all remaining in-flight requests when closing the reader or opening another comic in src/app/mod.rs

**Checkpoint**: User Story 3 is complete when rapid navigation prioritizes the new active page and stale background results are harmless.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validate performance, boundaries, and documentation across all stories.

- [x] T037 [P] Update reader prefetch quickstart with observed behavior and known limitations in specs/005-prefetch-vram-cache/quickstart.md
- [x] T038 [P] Add comments for non-obvious cancellation/generation logic in src/app/mod.rs
- [x] T039 Run focused validation commands from specs/005-prefetch-vram-cache/quickstart.md
- [x] T040 Run full validation commands documented in specs/005-prefetch-vram-cache/quickstart.md
- [ ] T041 Manually validate adjacent page turn latency, rapid navigation cancellation, two-page mode, and corrupt page recovery using specs/005-prefetch-vram-cache/quickstart.md

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies.
- **Foundational (Phase 2)**: Depends on Setup completion and blocks all user stories.
- **User Story 1 (Phase 3)**: Depends on Foundational phase.
- **User Story 2 (Phase 4)**: Depends on Foundational phase; practically benefits from US1 dispatch but reconciliation can be tested with injected worker results.
- **User Story 3 (Phase 5)**: Depends on Foundational phase; can be developed after or alongside US1/US2 once request tracking exists.
- **Polish (Phase 6)**: Depends on desired stories being complete.

### User Story Dependencies

- **US1 Nearby Pages Open Instantly**: MVP. No dependencies on US2 or US3.
- **US2 Background Results Become Displayable Cache Entries**: Can use injected results independently, then integrates with US1 dispatch.
- **US3 Stale Work Is Cancelled During Fast Navigation**: Depends on foundational in-flight tracking; validates cancellation across US1/US2 paths.

### Parallel Opportunities

- T002 and T003 can run in parallel after T001.
- T008 and T009 can run in parallel with T004-T007 once types are agreed.
- US1 tests T011 and T012 can run in parallel.
- US2 tests T019, T020, and T021 can run in parallel.
- US3 tests T029, T030, and T031 can run in parallel.
- Polish documentation tasks T037 and T038 can run in parallel after implementation stabilizes.

---

## Parallel Example: User Story 1

```bash
Task: "Add dispatch test for next, following, and previous page submissions in tests/app_routing.rs"
Task: "Add dispatch exclusion test for cached, queued, in-flight, and out-of-range pages in tests/prefetch_scheduler.rs"
```

## Parallel Example: User Story 2

```bash
Task: "Add reconciliation test that inserts successful prefetch results into the page texture cache in tests/app_routing.rs"
Task: "Add recoverable failed-prefetch result test that removes in-flight tracking without blocking visible page state in tests/app_routing.rs"
```

## Parallel Example: User Story 3

```bash
Task: "Add cancellation test for stale in-flight requests after a large page jump in tests/app_routing.rs"
Task: "Add late cancelled-result test that verifies stale results do not insert into PageTextureCache in tests/app_routing.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 setup.
2. Complete Phase 2 foundational session state and worker/cache ownership.
3. Complete Phase 3 dispatch logic.
4. Validate that `[current + 1, current + 2, current - 1]` requests are submitted and bounded.
5. Stop and confirm the reader remains responsive before adding reconciliation.

### Incremental Delivery

1. Add session-scoped runtime state and cache ownership.
2. Deliver US1 dispatch so background work starts.
3. Deliver US2 reconciliation so background work becomes display-ready cache entries.
4. Deliver US3 cancellation so rapid navigation prioritizes the user's current location.
5. Run polish validation and manual quickstart.

### Notes

- Keep archive reads and image decoding off the egui render loop for prefetch paths.
- Keep texture upload and TextureHandle insertion on the egui thread.
- Keep cache capacity bounded by PageTextureCache limits.
- Do not bypass VFS readers for CBZ, CBR, or PDF page bytes.
- Preserve direct visible-page failure behavior for corrupt pages.
