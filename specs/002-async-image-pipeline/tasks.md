# Tasks: Asynchronous Image Pipeline

**Input**: Design documents from `/specs/002-async-image-pipeline/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/async-image-pipeline.md, quickstart.md

**Prerequisite Note**: `.specify/scripts/bash/check-prerequisites.sh --json`
reported `ERROR: Not on a feature branch. Current branch: HEAD` because the Git
repository has no current commit. Tasks are generated from the active feature
directory `specs/002-async-image-pipeline`.

**Tests**: Included because the feature specification defines independent tests
for each user story and quickstart validation expects focused `cargo test`
coverage for decoding, prefetch scheduling, and LRU cache behavior.

**Organization**: Tasks are grouped by user story so each story can be
implemented and tested independently after shared foundations are complete.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel because it touches different files or depends
  only on completed prerequisites
- **[Story]**: User story label, required only for user story phases
- Every task includes an exact repository-relative file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add dependencies and create module/test skeletons for decode,
cache, and viewer scheduling work.

- [X] T001 Add eframe, image, crossbeam-channel, and lru dependencies in Cargo.toml
- [X] T002 Create decode module exports in src/decode/mod.rs
- [X] T003 Create cache module exports in src/cache/mod.rs
- [X] T004 Create viewer module exports in src/viewer/mod.rs
- [X] T005 Register decode, cache, and viewer modules from the crate root in src/lib.rs
- [X] T006 [P] Create placeholder decode pipeline integration test module in tests/decode_pipeline.rs
- [X] T007 [P] Create placeholder prefetch scheduler integration test module in tests/prefetch_scheduler.rs
- [X] T008 [P] Create placeholder page cache integration test module in tests/page_cache.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define shared request/result types, error contracts, cache bounds,
and public module exports required by all user stories.

**Critical**: No user story implementation should begin until this phase is
complete.

- [X] T009 Define DecodeRequestId, DecodePurpose, DecodeRequest, and DecodeResult types in src/decode/pipeline.rs
- [X] T010 Define DecodeError, WorkerError, and CacheError recoverable error enums in src/decode/error.rs
- [X] T011 [P] Define DEFAULT_PAGE_CACHE_CAPACITY and MAX_PAGE_CACHE_CAPACITY constants in src/cache/page_cache.rs
- [X] T012 [P] Define PrefetchState type with cached, queued, and in-flight page sets in src/viewer/prefetch.rs
- [X] T013 Wire shared decode, cache, and viewer types through src/decode/mod.rs, src/cache/mod.rs, and src/viewer/mod.rs

**Checkpoint**: Shared contracts are ready for independently testable user story
implementation.

---

## Phase 3: User Story 1 - Decode Pages Away From Reading Interaction (Priority: P1) MVP

**Goal**: Decode raw page bytes into display-ready page images on dedicated
background workers without blocking reading interaction.

**Independent Test**: Submit raw page bytes for decoding while simulating page
state updates, then confirm decoded results are returned asynchronously and
corrupt bytes return recoverable failures.

### Tests for User Story 1

- [X] T014 [P] [US1] Add valid in-memory image decode test in tests/decode_pipeline.rs
- [X] T015 [P] [US1] Add corrupt image bytes recoverable failure test in tests/decode_pipeline.rs
- [X] T016 [P] [US1] Add optional target-size downsampling test in tests/decode_pipeline.rs
- [X] T017 [P] [US1] Add worker pool processes 25 queued decode requests without blocking submission test in tests/decode_pipeline.rs
- [X] T018 [P] [US1] Add worker result preserves request_id and page_index test in tests/decode_pipeline.rs

### Implementation for User Story 1

- [X] T019 [US1] Implement raw bytes to egui::ColorImage decode_page function in src/decode/pipeline.rs
- [X] T020 [US1] Implement optional high-quality target-size downsampling before ColorImage conversion in src/decode/pipeline.rs
- [X] T021 [US1] Implement WorkerPool start, submit, try_recv, and shutdown with crossbeam channels in src/decode/worker.rs
- [X] T022 [US1] Implement worker request identity preservation and non-panicking decode failure delivery in src/decode/worker.rs
- [X] T023 [US1] Export decode_page, WorkerPool, DecodeRequest, DecodeResult, DecodePurpose, and errors in src/decode/mod.rs

**Checkpoint**: User Story 1 is functional and testable with `cargo test --test decode_pipeline`.

---

## Phase 4: User Story 2 - Prefetch Nearby Pages (Priority: P2)

**Goal**: Schedule nearby page decode candidates in the requested order while
skipping out-of-range, cached, queued, and in-flight pages.

**Independent Test**: Set current page and page state, then confirm candidates
are returned as `n+1`, `n+2`, `n-1` after filtering.

### Tests for User Story 2

- [X] T024 [P] [US2] Add middle-page prefetch order test for n+1, n+2, n-1 in tests/prefetch_scheduler.rs
- [X] T025 [P] [US2] Add first-page and last-page out-of-range filtering tests in tests/prefetch_scheduler.rs
- [X] T026 [P] [US2] Add cached page duplicate suppression test in tests/prefetch_scheduler.rs
- [X] T027 [P] [US2] Add queued and in-flight duplicate suppression tests in tests/prefetch_scheduler.rs
- [X] T028 [P] [US2] Add stale request generation filtering test in tests/prefetch_scheduler.rs

### Implementation for User Story 2

- [X] T029 [US2] Implement deterministic prefetch_candidates function in src/viewer/prefetch.rs
- [X] T030 [US2] Implement duplicate filtering against cached, queued, and in-flight sets in src/viewer/prefetch.rs
- [X] T031 [US2] Implement page generation marker and stale result helper in src/viewer/prefetch.rs
- [X] T032 [US2] Export PrefetchState, prefetch_candidates, and stale result helpers in src/viewer/mod.rs

**Checkpoint**: User Story 2 is functional and testable with `cargo test --test prefetch_scheduler`.

---

## Phase 5: User Story 3 - Promote Finished Pages Into the Display Cache (Priority: P3)

**Goal**: Provide bounded LRU cache behavior for display resources that are
created on the main thread after successful decode results.

**Independent Test**: Insert and access page display resources, then confirm
capacity limits, reuse, recency refresh, and least-recently-used eviction.

### Tests for User Story 3

- [X] T033 [P] [US3] Add default cache capacity and max capacity validation tests in tests/page_cache.rs
- [X] T034 [P] [US3] Add cache insert and contains/get reuse test in tests/page_cache.rs
- [X] T035 [P] [US3] Add least-recently-used eviction test in tests/page_cache.rs
- [X] T036 [P] [US3] Add cache access refreshes recency test in tests/page_cache.rs
- [X] T037 [P] [US3] Add cache never exceeds configured capacity test in tests/page_cache.rs

### Implementation for User Story 3

- [X] T038 [US3] Implement generic PageTextureCache<T> constructor and capacity validation in src/cache/page_cache.rs
- [X] T039 [US3] Implement PageTextureCache<T> get, contains, len, and capacity methods in src/cache/page_cache.rs
- [X] T040 [US3] Implement PageTextureCache<T> insert with strict LRU eviction in src/cache/page_cache.rs
- [X] T041 [US3] Add documentation comments clarifying that egui::TextureHandle insertion occurs only on the main thread in src/cache/page_cache.rs
- [X] T042 [US3] Export PageTextureCache and cache constants in src/cache/mod.rs

**Checkpoint**: User Story 3 is functional and testable with `cargo test --test page_cache`.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validate the whole feature, keep boundaries clear, and align docs
with the implemented commands.

- [X] T043 [P] Run cargo fmt and fix formatting issues across src/ and tests/
- [X] T044 [P] Run cargo clippy --all-targets -- -D warnings and address actionable warnings in src/ and tests/
- [X] T045 Run cargo test and confirm all feature and prior data-layer tests pass
- [X] T046 Verify decode workers do not create egui::TextureHandle and do not import LibraryService directly in src/decode/
- [X] T047 Verify viewer/cache modules do not bypass ArchiveReader for page bytes in src/viewer/ and src/cache/
- [X] T048 Validate quickstart.md instructions against implemented commands in specs/002-async-image-pipeline/quickstart.md
- [X] T049 Update specs/002-async-image-pipeline/quickstart.md if dependency or validation commands differ from implementation

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies; can start immediately.
- **Foundational (Phase 2)**: Depends on Phase 1; blocks all user stories.
- **User Story 1 (Phase 3)**: Depends on Phase 2; suggested MVP.
- **User Story 2 (Phase 4)**: Depends on Phase 2 and can run independently with page state fixtures.
- **User Story 3 (Phase 5)**: Depends on Phase 2 and can run independently with generic cache values.
- **Polish (Phase 6)**: Depends on desired user stories being complete.

### User Story Dependencies

- **US1 (P1)**: No dependency on US2 or US3 after foundations.
- **US2 (P2)**: No dependency on US1 implementation; it only needs shared page/request state concepts.
- **US3 (P3)**: No dependency on US1 or US2; it uses generic cache values and later accepts texture handles from main-thread integration.

### Within Each User Story

- Write tests before implementation tasks for that story.
- Implement shared models/contracts before services.
- Keep worker code free of texture-handle creation.
- Keep cache code generic so tests can validate LRU behavior without an egui runtime.
- Validate each story independently before moving to the next priority if working sequentially.

## Parallel Opportunities

- T006, T007, and T008 can run in parallel after module skeleton decisions.
- T011 and T012 can run in parallel because they touch different modules.
- T014 through T018 can be drafted in parallel with clear ownership of separate test cases in tests/decode_pipeline.rs.
- T024 through T028 can be drafted in parallel with clear ownership of separate test cases in tests/prefetch_scheduler.rs.
- T033 through T037 can be drafted in parallel with clear ownership of separate test cases in tests/page_cache.rs.
- After Phase 2, US1, US2, and US3 can be implemented in parallel because their primary write scopes are src/decode/, src/viewer/prefetch.rs, and src/cache/page_cache.rs.

## Parallel Example: User Story 1

```bash
Task: "Add valid in-memory image decode test in tests/decode_pipeline.rs"
Task: "Add corrupt image bytes recoverable failure test in tests/decode_pipeline.rs"
Task: "Add worker pool processes 25 queued decode requests without blocking submission test in tests/decode_pipeline.rs"
```

## Parallel Example: User Story 2

```bash
Task: "Add middle-page prefetch order test for n+1, n+2, n-1 in tests/prefetch_scheduler.rs"
Task: "Add first-page and last-page out-of-range filtering tests in tests/prefetch_scheduler.rs"
Task: "Add queued and in-flight duplicate suppression tests in tests/prefetch_scheduler.rs"
```

## Parallel Example: User Story 3

```bash
Task: "Add default cache capacity and max capacity validation tests in tests/page_cache.rs"
Task: "Add least-recently-used eviction test in tests/page_cache.rs"
Task: "Implement PageTextureCache<T> insert with strict LRU eviction in src/cache/page_cache.rs"
```

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 setup.
2. Complete Phase 2 foundations.
3. Complete Phase 3 decode pipeline and worker pool.
4. Stop and validate with `cargo test --test decode_pipeline`.

### Incremental Delivery

1. Finish Setup and Foundational phases.
2. Deliver US1 for background decode worker results.
3. Deliver US2 for deterministic smart prefetch scheduling.
4. Deliver US3 for bounded main-thread display cache behavior.
5. Run Phase 6 checks and quickstart validation.

### Parallel Team Strategy

1. Complete Setup and Foundational phases together.
2. Assign US1 to decode workers, US2 to prefetch scheduling, and US3 to LRU cache behavior.
3. Keep each story independently testable using the story-specific test command.
4. Integrate through the public exports in src/decode/mod.rs, src/viewer/mod.rs, and src/cache/mod.rs.

## Notes

- `[P]` tasks indicate safe parallel opportunities, but tasks editing the same
  file still need coordination.
- Worker code returns `egui::ColorImage`; it must not create `egui::TextureHandle`.
- Texture handles are main-thread display resources and belong in ViewerState
  integration, not background workers.
- Page bytes must come from existing VFS readers; this feature must not add
  alternate archive access.
