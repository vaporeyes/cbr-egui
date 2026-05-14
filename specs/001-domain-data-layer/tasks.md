# Tasks: Domain Models & Data Layer

**Input**: Design documents from `/specs/001-domain-data-layer/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/domain-data-layer.md, quickstart.md

**Prerequisite Note**: `.specify/scripts/bash/check-prerequisites.sh --json`
reported `ERROR: Not on a feature branch. Current branch: HEAD` because the Git
repository has no current commit. Tasks are generated from the active feature
directory `specs/001-domain-data-layer`.

**Tests**: Included because the feature specification defines mandatory
independent tests for each user story and quickstart validation expects focused
`cargo test` coverage.

**Organization**: Tasks are grouped by user story so each story can be
implemented and tested independently after shared foundations are complete.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel because it touches different files or depends
  only on completed prerequisites
- **[Story]**: User story label, required only for user story phases
- Every task includes an exact repository-relative file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add crate dependencies and create the module/test skeleton used by
all data-layer work.

- [X] T001 Add rusqlite, zip, quick-xml, serde, natord, thiserror, and tempfile dependencies in Cargo.toml
- [X] T002 Create library module exports in src/library/mod.rs
- [X] T003 Create VFS module exports in src/vfs/mod.rs
- [X] T004 [P] Create placeholder library storage integration test module in tests/library_storage.rs
- [X] T005 [P] Create placeholder metadata parser integration test module in tests/metadata_parser.rs
- [X] T006 [P] Create placeholder archive VFS integration test module in tests/archive_vfs.rs
- [X] T007 Register library and vfs modules from the crate root in src/lib.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define shared contracts, errors, and pure utilities that every user
story relies on.

**Critical**: No user story implementation should begin until this phase is
complete.

- [X] T008 Define Comic, ComicInput, Folder, Progress, ComicMetadata, and ArchivePage domain structs in src/library/models.rs
- [X] T009 Define LibraryError, MetadataError, and ArchiveError recoverable error enums in src/library/errors.rs
- [X] T010 Define the ArchiveReader trait and archive-reader result types in src/vfs/archive.rs
- [X] T011 [P] Implement page image extension detection and hidden metadata directory filtering helpers in src/vfs/ordering.rs
- [X] T012 [P] Implement natural page path ordering helper in src/vfs/ordering.rs
- [X] T013 Wire shared models and errors through public module exports in src/library/mod.rs and src/vfs/mod.rs

**Checkpoint**: Shared contracts are ready for independently testable user story
implementation.

---

## Phase 3: User Story 1 - Index Comic Library Records (Priority: P1) MVP

**Goal**: Initialize persistent storage for comics, folders, and progress, then
create/read/update records without viewer UI dependencies.

**Independent Test**: Create a temporary library database with nested folders
and two comics, save progress twice for one comic, and confirm stored records
read back with correct relationships and a single latest progress row.

### Tests for User Story 1

- [X] T014 [P] [US1] Add schema initialization and table existence tests in tests/library_storage.rs
- [X] T015 [P] [US1] Add folder creation and root/nested parent relationship tests in tests/library_storage.rs
- [X] T016 [P] [US1] Add comic upsert, duplicate path, and retrieval tests in tests/library_storage.rs
- [X] T017 [P] [US1] Add progress upsert and no-duplicate progress row tests in tests/library_storage.rs
- [X] T018 [P] [US1] Add 1,000 comic persistence/readback smoke test in tests/library_storage.rs

### Implementation for User Story 1

- [X] T019 [US1] Implement SQLite schema initialization for comics, folders, metadata, and progress tables in src/library/storage.rs
- [X] T020 [US1] Implement folder insert and retrieval storage functions in src/library/storage.rs
- [X] T021 [US1] Implement comic upsert by unique path and comic retrieval storage functions in src/library/storage.rs
- [X] T022 [US1] Implement progress save upsert and progress retrieval storage functions in src/library/storage.rs
- [X] T023 [US1] Implement LibraryService initialization, create_folder, upsert_comic, get_comic, save_progress, and get_progress in src/library/service.rs
- [X] T024 [US1] Expose LibraryService through src/library/mod.rs without any egui or ViewerState dependency

**Checkpoint**: User Story 1 is functional and testable with `cargo test --test library_storage`.

---

## Phase 4: User Story 2 - Read Embedded Comic Metadata (Priority: P2)

**Goal**: Extract target ComicInfo.xml fields from archive entries while keeping
missing or malformed metadata recoverable.

**Independent Test**: Use an archive reader fixture with valid, missing,
partial, and malformed ComicInfo.xml content and verify parsing behavior does
not block page listing.

### Tests for User Story 2

- [X] T025 [P] [US2] Add valid ComicInfo.xml field extraction test in tests/metadata_parser.rs
- [X] T026 [P] [US2] Add partial and missing optional ComicInfo.xml field tests in tests/metadata_parser.rs
- [X] T027 [P] [US2] Add malformed ComicInfo.xml recoverable error test in tests/metadata_parser.rs
- [X] T028 [P] [US2] Add missing ComicInfo.xml archive metadata test in tests/metadata_parser.rs
- [X] T029 [P] [US2] Add metadata failure does not block page listing test in tests/metadata_parser.rs

### Implementation for User Story 2

- [X] T030 [US2] Implement serde-compatible ComicInfo.xml DTO for Title, Number, Writer, and Penciller in src/library/metadata.rs
- [X] T031 [US2] Implement parse_comic_info_xml in src/library/metadata.rs
- [X] T032 [US2] Implement read_archive_metadata using ArchiveReader::read_entry for ComicInfo.xml in src/library/metadata.rs
- [X] T033 [US2] Map XML and archive entry failures into MetadataError variants in src/library/metadata.rs
- [X] T034 [US2] Export metadata parser APIs through src/library/mod.rs

**Checkpoint**: User Story 2 is functional and testable with `cargo test --test metadata_parser`.

---

## Phase 5: User Story 3 - List and Read Archive Pages (Priority: P3)

**Goal**: Provide a common archive interface for ZIP and RAR page listing and
on-demand page-byte retrieval with natural ordering and filtering.

**Independent Test**: Use ZIP and RAR archives containing `page_1.jpg`,
`page_2.jpg`, and `page_10.jpg`, then verify natural order, hidden/non-page
filtering, and exact byte retrieval without extracting the full archive.

### Tests for User Story 3

- [X] T035 [P] [US3] Add natural ordering unit coverage for one-, two-, and three-digit page names in tests/archive_vfs.rs
- [X] T036 [P] [US3] Add hidden metadata directory and non-image filtering tests in tests/archive_vfs.rs
- [X] T037 [P] [US3] Add ZIP page listing order test with page_1, page_2, and page_10 fixture entries in tests/archive_vfs.rs
- [X] T038 [P] [US3] Add ZIP on-demand page byte retrieval test in tests/archive_vfs.rs
- [X] T039 [P] [US3] Add ZIP missing page recoverable error test in tests/archive_vfs.rs
- [X] T040 [P] [US3] Add RAR reader contract test or backend-unavailable recoverable error test in tests/archive_vfs.rs

### Implementation for User Story 3

- [X] T041 [US3] Implement ArchiveReader shared filtering and sorted page construction helpers in src/vfs/archive.rs
- [X] T042 [US3] Implement ZipArchiveReader list_pages, read_page, and read_entry in src/vfs/zip.rs
- [X] T043 [US3] Implement RarArchiveReader list_pages, read_page, and read_entry using the selected native backend in src/vfs/rar.rs
- [X] T044 [US3] Map ZIP and RAR backend failures into ArchiveError variants in src/vfs/archive.rs
- [X] T045 [US3] Export ZipArchiveReader and RarArchiveReader through src/vfs/mod.rs

**Checkpoint**: User Story 3 is functional and testable with `cargo test --test archive_vfs`.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validate the full data layer, clean up public contracts, and ensure
the implementation remains aligned with the constitution.

- [X] T046 [P] Run cargo fmt and fix formatting issues across src/ and tests/
- [X] T047 [P] Run cargo clippy and address actionable warnings in src/ and tests/
- [X] T048 Run cargo test and confirm all feature tests pass
- [X] T049 Verify no library or VFS module imports egui, eframe, or ViewerState in src/library/ and src/vfs/
- [X] T050 Validate quickstart.md instructions against the implemented commands in specs/001-domain-data-layer/quickstart.md
- [X] T051 Update specs/001-domain-data-layer/quickstart.md if native RAR backend setup differs from the implementation

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies; can start immediately.
- **Foundational (Phase 2)**: Depends on Phase 1; blocks all user stories.
- **User Story 1 (Phase 3)**: Depends on Phase 2; suggested MVP.
- **User Story 2 (Phase 4)**: Depends on Phase 2 and can run independently with a mock ArchiveReader fixture, then integrate with VFS readers when US3 is complete.
- **User Story 3 (Phase 5)**: Depends on Phase 2 and can run independently of library storage.
- **Polish (Phase 6)**: Depends on desired user stories being complete.

### User Story Dependencies

- **US1 (P1)**: No dependency on other stories after foundations.
- **US2 (P2)**: No dependency on US1; depends only on the ArchiveReader trait and can use test fixtures.
- **US3 (P3)**: No dependency on US1 or US2; implements the concrete archive readers used by later integration.

### Within Each User Story

- Write tests before implementation tasks for that story.
- Implement models/contracts before service/storage behavior.
- Keep LibraryService and VFS APIs free of egui or ViewerState dependencies.
- Validate each story independently before moving to the next priority if working sequentially.

## Parallel Opportunities

- T004, T005, and T006 can run in parallel after module skeleton decisions.
- T011 and T012 can run in parallel because they are pure helper functions in the same module only if coordinated; otherwise run sequentially to avoid edit conflicts.
- T014 through T018 can be drafted in parallel with clear ownership of separate test cases in tests/library_storage.rs.
- T025 through T029 can be drafted in parallel with clear ownership of separate test cases in tests/metadata_parser.rs.
- T035 through T040 can be drafted in parallel with clear ownership of separate test cases in tests/archive_vfs.rs.
- After Phase 2, US1, US2, and US3 can be implemented in parallel because their write scopes are mostly src/library/storage.rs, src/library/metadata.rs, and src/vfs/*.rs respectively.

## Parallel Example: User Story 1

```bash
Task: "Add schema initialization and table existence tests in tests/library_storage.rs"
Task: "Add folder creation and root/nested parent relationship tests in tests/library_storage.rs"
Task: "Add comic upsert, duplicate path, and retrieval tests in tests/library_storage.rs"
Task: "Add progress upsert and no-duplicate progress row tests in tests/library_storage.rs"
```

## Parallel Example: User Story 2

```bash
Task: "Add valid ComicInfo.xml field extraction test in tests/metadata_parser.rs"
Task: "Add malformed ComicInfo.xml recoverable error test in tests/metadata_parser.rs"
Task: "Implement parse_comic_info_xml in src/library/metadata.rs"
```

## Parallel Example: User Story 3

```bash
Task: "Add natural ordering unit coverage for one-, two-, and three-digit page names in tests/archive_vfs.rs"
Task: "Implement ZipArchiveReader list_pages, read_page, and read_entry in src/vfs/zip.rs"
Task: "Implement RarArchiveReader list_pages, read_page, and read_entry using the selected native backend in src/vfs/rar.rs"
```

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 setup.
2. Complete Phase 2 foundations.
3. Complete Phase 3 storage tests and LibraryService implementation.
4. Stop and validate with `cargo test --test library_storage`.

### Incremental Delivery

1. Finish Setup and Foundational phases.
2. Deliver US1 for persistent library records and progress.
3. Deliver US2 for ComicInfo.xml metadata parsing.
4. Deliver US3 for ZIP/RAR archive page VFS behavior.
5. Run Phase 6 checks and quickstart validation.

### Parallel Team Strategy

1. Complete Setup and Foundational phases together.
2. Assign US1 to library storage, US2 to metadata parsing, and US3 to archive VFS.
3. Keep each story independently testable using the story-specific test command.
4. Integrate through the public exports in src/library/mod.rs and src/vfs/mod.rs.

## Notes

- `[P]` tasks indicate safe parallel opportunities, but tasks editing the same
  file still need coordination.
- All archive and metadata failures must remain recoverable errors.
- Do not introduce UI state, egui calls, eframe dependencies, decode caches, or
  ViewerState code in this feature.
- Do not perform normal full-archive extraction to disk for ZIP or RAR page
  access.
