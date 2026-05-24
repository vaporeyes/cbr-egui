# Tasks: State Persistence & Lifecycle Management

**Input**: Design documents from `specs/007-state-persistence-lifecycle/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/lifecycle-settings.md, quickstart.md

**Tests**: Included because the feature spec and quickstart define independent automated validation for each user story.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm current lifecycle, config, and service seams before feature work.

- [X] T001 Review existing config defaults and serialization behavior in `src/config.rs` and `tests/app_config.rs`
- [X] T002 Review app wrapper construction and `eframe::App` implementation in `src/app/ui.rs`
- [X] T003 [P] Review LibraryService progress and resume APIs in `src/library/service.rs` and `src/library/storage.rs`
- [X] T004 [P] Review current startup construction in `src/main.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared config and lifecycle helpers required by all user stories.

**CRITICAL**: No user story work can begin until this phase is complete.

- [X] T005 Add safe zoom sensitivity normalization/clamping helpers for `AppConfig` in `src/config.rs`
- [X] T006 Add default app config path helper in `src/config.rs`
- [X] T007 Add `SettingsWindowState` and lifecycle error/status fields to `EguiComicReaderApp` in `src/app/ui.rs`
- [X] T008 Add constructors that inject `AppConfig`, config path, and `LibraryService` into `EguiComicReaderApp` in `src/app/ui.rs`
- [X] T009 Add helper to apply `AppConfig` to egui visuals and active viewer defaults in `src/app/ui.rs`

**Checkpoint**: App can own config/service lifecycle state without changing user-facing behavior.

---

## Phase 3: User Story 1 - Preserve Reading Progress on Exit (Priority: P1) MVP

**Goal**: Save the active comic, intended page, read status, and app config during the app lifecycle save hook.

**Independent Test**: Open a comic, move to a later page, trigger save, and verify config plus progress were persisted.

### Tests for User Story 1

- [X] T010 [P] [US1] Add app lifecycle config-save test in `tests/app_config.rs`
- [X] T011 [US1] Add active reading progress save test for current page in `tests/app_routing.rs`
- [X] T012 [US1] Add read-status save test for final page in `tests/app_routing.rs`
- [X] T013 [US1] Add save failure non-panic/status test in `tests/app_routing.rs`

### Implementation for User Story 1

- [X] T014 [US1] Add active-session progress snapshot helper to `ReadingSession` or `ComicReaderApp` in `src/app/mod.rs`
- [X] T015 [US1] Add progress flushing helper that writes through `LibraryService::save_progress` in `src/app/ui.rs`
- [X] T016 [US1] Implement `eframe::App::save` to save `AppConfig` and active progress in `src/app/ui.rs`
- [X] T017 [US1] Surface non-blocking lifecycle save errors in library and reader chrome via `src/app/ui.rs`
- [X] T018 [US1] Ensure save uses `current_page_index` even when the page is still loading in `src/app/ui.rs`

**Checkpoint**: User Story 1 works independently; lifecycle save persists config and active reading progress.

---

## Phase 4: User Story 2 - Resume Last Session on Startup (Priority: P2)

**Goal**: Load configuration and restore the last valid reading session before the first user-visible view.

**Independent Test**: Save progress for an available comic, construct the app through startup initialization, and verify the reader opens at the saved page.

### Tests for User Story 2

- [X] T019 [P] [US2] Add startup resume test for valid saved session in `tests/app_routing.rs`
- [X] T020 [US2] Add startup fallback test for no saved session in `tests/app_routing.rs`
- [X] T021 [US2] Add startup fallback test for unavailable saved comic in `tests/app_routing.rs`
- [X] T022 [US2] Add saved-page clamping test when progress exceeds page count in `tests/app_routing.rs`

### Implementation for User Story 2

- [X] T023 [US2] Add startup hydration helper that loads library items from `LibraryService` into `ComicReaderApp` in `src/app/ui.rs`
- [X] T024 [US2] Update `ComicReaderApp::resume_last_session` to clamp invalid saved pages and reject unavailable or zero-page comics in `src/app/mod.rs`
- [X] T025 [US2] Add `EguiComicReaderApp` startup constructor that applies config and attempts resume through `LibraryService` in `src/app/ui.rs`
- [X] T026 [US2] Modify native startup to load config, construct LibraryService, apply initial visuals, and use the startup constructor in `src/main.rs`
- [X] T027 [US2] Ensure resumed reading sessions inherit configured reading direction before page rendering in `src/app/ui.rs`

**Checkpoint**: User Stories 1 and 2 work independently; app can save a session and start directly in the reader when valid.

---

## Phase 5: User Story 3 - Adjust Reader Preferences (Priority: P3)

**Goal**: Provide a toolbar settings window for appearance, zoom sensitivity, and default reading direction with immediate application.

**Independent Test**: Open settings, change each preference, confirm the running app updates, save, restart, and confirm preferences persist.

### Tests for User Story 3

- [X] T028 [P] [US3] Add settings state open/close test in `tests/app_routing.rs`
- [X] T029 [US3] Add dark/light preference application test in `tests/app_routing.rs`
- [X] T030 [P] [US3] Add reading direction preference application test in `tests/spread_viewer.rs`
- [X] T031 [P] [US3] Add zoom sensitivity preference behavior test in `tests/viewer_interaction.rs`

### Implementation for User Story 3

- [X] T032 [US3] Add toolbar settings control in library and reader toolbar paths in `src/app/ui.rs`
- [X] T033 [US3] Implement `egui::Window` settings modal for dark mode, zoom sensitivity, and reading direction in `src/app/ui.rs`
- [X] T034 [US3] Apply dark/light visuals immediately when settings change in `src/app/ui.rs`
- [X] T035 [US3] Replace hard-coded zoom sensitivity with configurable viewer sensitivity in `src/viewer/state.rs` and `src/viewer/ui.rs`
- [X] T036 [US3] Apply default reading direction to newly opened and active reading sessions in `src/app/mod.rs` and `src/app/ui.rs`
- [X] T037 [US3] Persist settings changes through lifecycle save without resetting active page, cache, or prefetch state in `src/app/ui.rs`

**Checkpoint**: All user stories are independently functional and settings changes persist across restart.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validation, cleanup, and end-to-end lifecycle checks.

- [X] T038 [P] Update quickstart validation notes if implementation changes command names or settings behavior in `specs/007-state-persistence-lifecycle/quickstart.md`
- [X] T039 Run `cargo test --test app_routing --test app_config --test library_scanner` and fix failures in `src/app/ui.rs`, `src/app/mod.rs`, `src/config.rs`, or `src/library/service.rs`
- [X] T040 Run `cargo test --test viewer_interaction --test spread_viewer` and fix preference-related regressions in `src/viewer/state.rs`, `src/viewer/ui.rs`, or `src/viewer/spread.rs`
- [X] T041 Run `cargo clippy --all-targets -- -D warnings` and resolve warnings in `src/app/ui.rs`, `src/app/mod.rs`, `src/config.rs`, `src/main.rs`, or `src/viewer/ui.rs`
- [ ] T042 Manually validate close/reopen resume and settings persistence with a local comic library using `specs/007-state-persistence-lifecycle/quickstart.md`
- [X] T043 Confirm save/startup paths do not perform archive reads, image decoding, thumbnail generation, or scans in `src/app/ui.rs` and `src/main.rs`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies; can start immediately.
- **Foundational (Phase 2)**: Depends on Setup; blocks all user stories.
- **User Story 1 (Phase 3)**: Depends on Foundational; MVP scope.
- **User Story 2 (Phase 4)**: Depends on Foundational and benefits from US1 persisted progress, but startup fallback behavior can be tested independently.
- **User Story 3 (Phase 5)**: Depends on Foundational; can be implemented after US1/US2 or in parallel once constructors own config.
- **Polish (Phase 6)**: Depends on selected user stories being complete.

### User Story Dependencies

- **US1 Preserve Reading Progress**: No dependency on US2 or US3.
- **US2 Resume Last Session**: Uses progress records saved by US1 for full end-to-end validation; fallback and clamping logic can be tested with pre-seeded service data.
- **US3 Adjust Reader Preferences**: Uses foundational config ownership; independent of progress persistence except final persistence validation.

### Parallel Opportunities

- T003 and T004 can run in parallel during setup.
- T010 can run in parallel with the app-routing tests T011-T013 because it targets `tests/app_config.rs`.
- T019 can run in parallel with other story work once app-routing test edits are coordinated.
- T030 and T031 can run in parallel with T028-T029 because they target different test files.
- T038 can run in parallel with final code cleanup after behavior is stable.

---

## Parallel Example: User Story 1

```bash
Task: "Add app lifecycle config-save test in tests/app_config.rs"
Task: "Add active reading progress save test for current page in tests/app_routing.rs"
Task: "Add read-status save test for final page in tests/app_routing.rs"
Task: "Add save failure non-panic/status test in tests/app_routing.rs"
```

## Parallel Example: User Story 2

```bash
Task: "Add startup resume test for valid saved session in tests/app_routing.rs"
Task: "Add startup fallback test for no saved session in tests/app_routing.rs"
Task: "Add startup fallback test for unavailable saved comic in tests/app_routing.rs"
Task: "Add saved-page clamping test when progress exceeds page count in tests/app_routing.rs"
```

## Parallel Example: User Story 3

```bash
Task: "Add settings state open/close test in tests/app_routing.rs"
Task: "Add dark/light preference application test in tests/app_routing.rs"
Task: "Add reading direction preference application test in tests/spread_viewer.rs"
Task: "Add zoom sensitivity preference behavior test in tests/viewer_interaction.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational config/service ownership.
3. Complete Phase 3: lifecycle save for config and active progress.
4. Validate with US1 tests and focused manual close/save behavior.

### Incremental Delivery

1. Add US1 to prevent progress loss on exit.
2. Add US2 to consume saved progress on startup.
3. Add US3 to expose preferences and make config user-editable.
4. Run final quickstart and regression checks.

### Notes

- Tests should be written before implementation and fail for the missing behavior.
- Keep SQLite access behind LibraryService.
- Keep config file access in config/app lifecycle code, not ViewerState.
- Do not add archive reads, image decoding, thumbnail generation, or scans to save/startup settings paths.
