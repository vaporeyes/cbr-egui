# Feature Specification: Library Metadata Surfacing

**Feature Branch**: `008-library-metadata-surfacing`  
**Created**: 2026-05-14  
**Status**: Draft  
**Input**: User description: "Phase 8: Library Metadata Surfacing. The ComicInfo.xml parser accurately captures series, writer, and penciller data, but the UI renders a flat grid ignoring this metadata. Extend library grid items to include joined metadata subtitles, and add grouping/filtering by unique series or folder names."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See Metadata In Library Cards (Priority: P1)

As a reader browsing the library, I want each comic entry to show meaningful metadata such as issue number and creator names so that I can identify issues without opening each archive.

**Why this priority**: The existing parser already captures metadata, but the primary library view hides it. Showing this data makes the current collection immediately more useful.

**Independent Test**: Can be tested by scanning comics with known metadata, opening the library, and confirming each matching item shows a concise subtitle below its title.

**Acceptance Scenarios**:

1. **Given** a scanned comic with series, issue number, and writer metadata, **When** the library grid is shown, **Then** the item displays the title and a subtitle containing the issue number and writer.
2. **Given** a scanned comic with only partial metadata, **When** the library grid is shown, **Then** the subtitle omits missing fields without showing empty separators or placeholder text.
3. **Given** a scanned comic with no usable metadata, **When** the library grid is shown, **Then** the item remains visible and uses its current title-only presentation.

---

### User Story 2 - Filter Library By Series Or Folder (Priority: P2)

As a reader with a large library, I want to narrow the visible comics by series or folder so that I can quickly browse related issues without scrolling through the full collection.

**Why this priority**: Metadata is most valuable when it helps users navigate larger collections. A simple grouping filter reduces visual noise and supports common comic-reading workflows.

**Independent Test**: Can be tested by scanning a mixed library, selecting a series or folder filter, and confirming only matching comics remain visible while the original collection is restored when the filter is cleared.

**Acceptance Scenarios**:

1. **Given** a library containing multiple series, **When** the user selects one series, **Then** only comics from that series are displayed.
2. **Given** comics with no series metadata, **When** folder grouping is available, **Then** those comics can still be found through their containing folder.
3. **Given** an active filter, **When** the user clears the selection, **Then** all available comics return to the current library view.

---

### User Story 3 - Preserve Library Interactions While Filtered (Priority: P3)

As a reader using a filtered library, I want thumbnails, list view, opening comics, unavailable markers, and scan status to keep working normally so that filtering does not break existing library workflows.

**Why this priority**: Filtering is an overlay on the current library experience and must not regress existing interactions.

**Independent Test**: Can be tested by applying a filter and verifying thumbnails, list mode, comic opening, and unavailable item handling behave the same as in the unfiltered view.

**Acceptance Scenarios**:

1. **Given** an active filter in thumbnail view, **When** thumbnails finish loading, **Then** visible filtered items update with their covers.
2. **Given** an active filter in list view, **When** a visible comic is selected, **Then** the reader opens that comic.
3. **Given** an active filter and unavailable comics, **When** the filter includes unavailable entries, **Then** unavailable state remains visible and unavailable comics still cannot be opened.

### Edge Cases

- If multiple comics share the same series but use inconsistent casing or surrounding whitespace, they appear under one normalized group while retaining their original display text where shown.
- If metadata is missing, malformed, or incomplete, the library falls back to folder-based grouping and title-only display without hiding the comic.
- If a creator list is long, subtitles are shortened to fit the card or list row without overlapping other UI.
- If a scan updates metadata while a filter is active, the visible results refresh without losing the user's selected grouping mode unless the selected group no longer exists.
- If the library contains hundreds or thousands of comics, group selection and filtering remain responsive during normal browsing.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The library view MUST display a secondary subtitle for comics when metadata can produce a useful summary.
- **FR-002**: Subtitles MUST prefer issue number and primary writer when available, and MAY include penciller when writer is unavailable or when space permits.
- **FR-003**: Subtitle formatting MUST omit missing metadata cleanly and MUST NOT show empty labels, duplicate separators, or raw unknown values.
- **FR-004**: The library data used for grid and list rendering MUST include metadata fields associated with each comic when such metadata exists.
- **FR-005**: Users MUST be able to filter the library by a unique series name when series metadata exists.
- **FR-006**: Users MUST be able to filter the library by containing folder so comics without series metadata remain discoverable.
- **FR-007**: Users MUST be able to clear the active filter and return to the complete library view.
- **FR-008**: Filtering MUST apply consistently to thumbnail and list library views.
- **FR-009**: Filtering MUST NOT change comic open behavior, thumbnail loading behavior, unavailable comic behavior, or scan status reporting.
- **FR-010**: The active filter MUST remain stable across view mode changes within the current app session.
- **FR-011**: The library MUST handle metadata updates after a scan by refreshing available groups and visible items.
- **FR-012**: The feature MUST preserve reader responsiveness during browsing; metadata display and filtering MUST NOT cause noticeable stalls in the UI.

### Key Entities *(include if feature involves data)*

- **Library Item**: A comic entry displayed in the library. Key attributes include comic identifier, title, source path, page count, availability, thumbnail status, and optional metadata subtitle.
- **Comic Metadata**: Descriptive information extracted from a comic archive. Key attributes include series, issue number, writer, penciller, and other creator fields available for future display.
- **Library Group**: A selectable grouping value used for filtering. Key attributes include group type, display name, normalized key, and item count.
- **Active Library Filter**: The current user selection that determines which library items are visible. Key attributes include filter type and selected group key.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For comics with complete series, issue, and writer metadata, 95% of visible library items show a subtitle that includes issue and writer information.
- **SC-002**: Users can reduce a mixed 500-comic library to a single series or folder in under 3 seconds from the library screen.
- **SC-003**: Clearing a filter restores the full available library view in under 1 second for a 500-comic library.
- **SC-004**: In usability testing, users can identify the correct issue from a metadata-rich series without opening a comic at least 90% of the time.
- **SC-005**: Existing library actions, including opening a comic, switching thumbnail/list views, and showing unavailable items, continue to pass regression tests while a filter is active.

## Assumptions

- Series metadata is the primary grouping when available; folder grouping is the fallback and complementary navigation mode.
- "Issue number" refers to the issue or number field already extracted from comic metadata.
- The first listed writer is treated as the primary writer when multiple writers are present.
- The filter is session-scoped for this phase and does not need to persist across app restarts.
- This phase surfaces existing parsed metadata; editing metadata is out of scope.
