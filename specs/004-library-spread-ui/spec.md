# Feature Specification: Library Spread UI

**Feature Branch**: `004-library-spread-ui`  
**Created**: 2026-05-13  
**Status**: Draft  
**Input**: User description: "Two-Page Spread Logic. Add a toggle. If enabled, query the VFS for page dimensions. If width > height (a pre-stitched spread), render alone. If width <= height, request the next page and render side-by-side in an egui::Ui::horizontal layout. Phase 4: Library Management UI. Construct the navigational shell. Task 4.1: File System Watcher. Implement notify to scan a defined root directory, updating the SQLite database when new archives are added or removed. Task 4.2: Grid View. Build an egui::ScrollArea containing a responsive grid. Fetch cover images, thumbnail them to max 300px height, and cache them locally to disk to avoid re-extracting on startup. Task 4.3: Integration & State Routing. Implement a top-level state enum AppState with Library and Reading states. Route update loops based on this state to swap between the library view and viewer canvas."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Read With Optional Spreads (Priority: P1)

As a reader, I can turn on two-page spread mode so standard portrait pages are shown side-by-side while already-stitched landscape spreads remain centered as a single page.

**Why this priority**: Spread reading is the primary reader-facing enhancement and directly changes how pages are displayed.

**Independent Test**: Can be tested by opening a comic with portrait pages and a landscape spread, toggling spread mode, and verifying that portrait pairs display together while the landscape spread displays alone.

**Acceptance Scenarios**:

1. **Given** spread mode is disabled and a page is ready, **When** the reader views any page, **Then** the viewer displays only the current page.
2. **Given** spread mode is enabled and the current page is wider than it is tall, **When** the reader views that page, **Then** the viewer displays that page alone as a pre-stitched spread.
3. **Given** spread mode is enabled and the current page is not wider than it is tall, **When** the next page is available, **Then** the viewer displays the current page and next page side-by-side.
4. **Given** spread mode is enabled on the last page of a comic, **When** no next page exists, **Then** the viewer displays the current page alone without an error.

---

### User Story 2 - Keep Library Current Automatically (Priority: P2)

As a library user, I can choose a root folder and have the application keep the visible library in sync as archive files are added, removed, or changed.

**Why this priority**: A navigable library is only useful if it reflects the actual collection without manual rescans for common file changes.

**Independent Test**: Can be tested by setting a library root, adding and removing supported archive files, and verifying that the library updates without restarting the application.

**Acceptance Scenarios**:

1. **Given** a library root is configured, **When** a supported archive is added under that root, **Then** the comic appears in the library after scanning completes.
2. **Given** a known comic exists in the library, **When** its archive is removed from the root, **Then** the comic is removed or marked unavailable in the library view.
3. **Given** multiple file changes occur close together, **When** scanning completes, **Then** the library reflects the final folder contents without duplicate entries.

---

### User Story 3 - Browse Covers In A Responsive Grid (Priority: P3)

As a library user, I can browse my collection as a responsive cover grid so I can visually choose a comic and open it for reading.

**Why this priority**: Cover browsing provides the navigational shell requested for moving between library and reader views.

**Independent Test**: Can be tested by loading a collection with multiple comics, resizing the window, and verifying that covers wrap responsively and open the selected comic.

**Acceptance Scenarios**:

1. **Given** comics exist in the library, **When** the user opens the library view, **Then** each available comic is represented by a cover tile with a bounded thumbnail.
2. **Given** cached thumbnails already exist, **When** the application starts, **Then** the library grid uses cached thumbnails rather than re-extracting covers.
3. **Given** the user selects a comic tile, **When** the selection is accepted, **Then** the application switches to the reading view for that comic.
4. **Given** the user leaves the reader, **When** they return to the library, **Then** the application shows the library grid without losing the current collection.

### Edge Cases

- Spread mode is enabled but the next page is missing, corrupt, unsupported, or still loading.
- A pre-stitched spread appears at an odd or even page index and should still render alone based on its dimensions.
- The next page in a spread pair has a different aspect ratio or cannot be prepared.
- The configured library root is missing, inaccessible, empty, or moved while the application is running.
- File watcher events arrive in bursts or out of order during large copy, delete, or rename operations.
- A supported archive is corrupt, contains no usable cover page, or has hidden metadata directories before image pages.
- Thumbnail cache entries are stale because the source archive changed after the thumbnail was generated.
- Large collections and high-resolution covers must not make library navigation or reader interaction unresponsive.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The reader MUST provide a user-controlled spread-mode toggle that can be enabled and disabled while reading.
- **FR-002**: When spread mode is disabled, the reader MUST continue to display one current page at a time.
- **FR-003**: When spread mode is enabled, the reader MUST determine whether the current page is a pre-stitched spread by comparing page width and height.
- **FR-004**: When the current page is wider than it is tall, the reader MUST display the current page alone even when spread mode is enabled.
- **FR-005**: When the current page is not wider than it is tall and a next page exists, the reader MUST request and display the next page alongside the current page.
- **FR-006**: When a side-by-side page cannot be displayed, the reader MUST keep the current page readable and present the missing side as a recoverable loading or error state.
- **FR-007**: Page turns MUST reset zoom and pan for the new displayed page or spread.
- **FR-008**: The library MUST allow a root folder to define the collection to scan and watch.
- **FR-009**: The library MUST detect added, removed, and changed supported archive files under the root folder and update collection records accordingly.
- **FR-010**: Library updates MUST avoid duplicate comic records for the same archive path.
- **FR-011**: The library view MUST display comics in a responsive cover grid that adapts to available window width.
- **FR-012**: Cover thumbnails MUST be generated from the first usable page of each archive when no valid cached thumbnail exists.
- **FR-013**: Cover thumbnails MUST be constrained to a maximum display height of 300 pixels.
- **FR-014**: Cover thumbnails MUST be cached locally so normal startup can show existing covers without extracting the same archive pages again.
- **FR-015**: Thumbnail cache entries MUST be invalidated or refreshed when their source archive changes.
- **FR-016**: The application MUST route between a library browsing state and a comic reading state without restarting.
- **FR-017**: Selecting a comic from the library MUST open the reader for that comic.
- **FR-018**: The reader MUST provide a path back to the library view.
- **FR-019**: Reader features MUST keep library collection management separate from reading interaction state.
- **FR-020**: Archive-reading features MUST use the existing archive access capability without extracting archive contents into normal library folders.
- **FR-021**: Page-loading and thumbnail-loading features MUST define bounded cache and recovery behavior for corrupt or unsupported pages.

### Key Entities

- **Library Root**: The user-selected folder whose supported archive files form the visible collection.
- **Comic Record**: A library entry representing one archive, including identity, path, availability, metadata, and cover status.
- **Cover Thumbnail**: A locally cached, bounded image derived from a comic's first usable page and refreshed when the source archive changes.
- **Reading Session**: The active reader state for one comic, including current page, spread mode, page resources, and navigation state.
- **Spread Pair**: The display decision for a current page, representing either one pre-stitched page or a current-plus-next page pair.
- **Application State**: The top-level mode that determines whether the user is browsing the library or reading a selected comic.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can toggle spread mode and see the page layout update within one visible interaction cycle for already prepared pages.
- **SC-002**: In a test comic containing portrait pages and landscape spreads, 100% of landscape pages render alone and portrait pages render as pairs when a next page exists.
- **SC-003**: New supported archives added to the watched library root appear in the library within 5 seconds after the file is stable for typical local disk operations.
- **SC-004**: Removed archives disappear from the library or are marked unavailable within 5 seconds after removal is detected.
- **SC-005**: A library of 500 comics remains browsable, with cover tiles visible and scrolling responsive during normal use.
- **SC-006**: Cached cover thumbnails prevent repeated cover extraction for unchanged comics across application restarts.
- **SC-007**: Users can move from library browsing to reading and back to the library in no more than two explicit actions each way.
- **SC-008**: Reader interaction remains responsive during page navigation, with page turns, zooming, panning, and spread toggling staying visually smooth during normal use.

## Assumptions

- Supported library items are the same archive formats already supported by the existing collection features.
- The library root is a local or mounted filesystem path that can be scanned and watched by the application.
- The first usable page of an archive is an acceptable cover image unless richer metadata is added by a later feature.
- Thumbnail cache storage is local application data and can be rebuilt from source archives if missing or invalid.
- Spread mode pairs the current page with the immediately following page; advanced manga direction, cover-page pairing rules, and user-defined pairing offsets are out of scope for this feature.
- The navigational shell focuses on library browsing and reading; advanced filtering, search, collection editing, and multi-root management are out of scope for this feature.
