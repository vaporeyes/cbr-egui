# Feature Specification: Continuous Vertical Scroll

**Feature Branch**: `006-continuous-scroll`  
**Created**: 2026-05-14  
**Status**: Draft  
**Input**: User description: "Add continuous vertical reading so pages appear in one scrollable column, only visible and nearby pages are prepared for display, and unknown page sizes use stable placeholders until actual dimensions are available."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Read Continuously (Priority: P1)

As a reader, I can switch to a continuous vertical layout and scroll through pages in one uninterrupted column instead of turning one page at a time.

**Why this priority**: This is the core user value: continuous reading must exist before performance optimizations or placeholder behavior matter.

**Independent Test**: Can be tested by opening a multi-page comic, enabling continuous vertical reading, and scrolling from the first page into later pages without using next/previous navigation.

**Acceptance Scenarios**:

1. **Given** a readable comic with multiple pages, **When** the reader enables continuous vertical layout, **Then** pages appear in a single vertical reading flow.
2. **Given** continuous vertical layout is active, **When** the reader scrolls down through the comic, **Then** the visible pages advance according to scroll position without requiring explicit page turns.
3. **Given** continuous vertical layout is active, **When** the reader returns to paged layout, **Then** the reader returns to a discrete page view at the nearest currently visible page.

---

### User Story 2 - Keep Scrolling Responsive (Priority: P2)

As a reader, I can scroll through large comics without the interface stalling because only visible and nearby pages are prepared for display.

**Why this priority**: Continuous layouts can involve many pages; responsiveness depends on limiting work to the viewport region.

**Independent Test**: Can be tested with a large archive by scrolling rapidly from early pages to later pages and verifying the interface remains responsive while pages load progressively near the viewport.

**Acceptance Scenarios**:

1. **Given** a large comic in continuous vertical layout, **When** the reader scrolls rapidly through the document, **Then** the interface remains responsive and does not try to prepare every page at once.
2. **Given** pages above and below the viewport are outside the near-visible margin, **When** continuous layout calculates visible content, **Then** those distant pages are not requested for display preparation.
3. **Given** a page enters the visible area or near-visible margin, **When** its texture is not already available, **Then** it is queued for preparation without blocking scrolling.

---

### User Story 3 - Stable Layout While Sizes Become Known (Priority: P3)

As a reader, I can start scrolling before every page size is known, and the layout settles as actual page dimensions are discovered without disruptive jumps.

**Why this priority**: Page dimensions may arrive over time; a usable placeholder strategy prevents blank or unstable continuous layouts during loading.

**Independent Test**: Can be tested by opening a comic whose later pages have not yet loaded and verifying placeholder regions reserve reasonable space until real page sizes are known.

**Acceptance Scenarios**:

1. **Given** only the first page dimensions are known, **When** continuous vertical layout is shown, **Then** later pages reserve placeholder space using the first page's aspect ratio.
2. **Given** an actual page size becomes available, **When** the layout is recalculated, **Then** the page's reserved height updates to match its real dimensions.
3. **Given** a page size changes from placeholder to actual dimensions, **When** the reader is already scrolling, **Then** the scroll position remains close to the same reading location.

### Edge Cases

- The comic has zero readable pages or all pages fail to load.
- The first page fails before an aspect ratio is available for placeholders.
- Pages have mixed portrait, landscape, square, or unusually tall dimensions.
- The reader scrolls rapidly from the beginning to the end before nearby pages finish loading.
- A page near the viewport is corrupt or unsupported.
- Two-page spread mode and continuous vertical layout are both requested.
- The window is resized while continuous layout is active.
- A large archive contains hundreds or thousands of pages.
- Page preparation completes after the page is no longer near the visible viewport.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Users MUST be able to enable and disable continuous vertical reading from the reader interface.
- **FR-002**: When continuous vertical reading is enabled, the reader MUST present pages as a single vertically scrollable flow.
- **FR-003**: The reader MUST calculate a total scrollable height for the continuous flow using known page dimensions and placeholder dimensions for unknown pages.
- **FR-004**: The reader MUST determine which pages intersect the visible viewport.
- **FR-005**: The reader MUST include a one-page margin above and below the visible viewport when deciding which pages are eligible for display preparation.
- **FR-006**: The reader MUST request or display only pages that intersect the visible viewport or the one-page margin, unless another reader feature explicitly needs a page.
- **FR-007**: The reader MUST reuse already prepared display entries when a visible page has already been prepared.
- **FR-008**: The reader MUST prepare missing near-visible pages in the background without blocking scrolling.
- **FR-009**: The reader MUST reserve placeholder regions for pages whose actual dimensions are not yet known.
- **FR-010**: Placeholder regions MUST use a reasonable aspect ratio derived from the first known page; if no page size is known, the reader MUST use a stable default portrait ratio.
- **FR-011**: The reader MUST update the continuous layout when actual page dimensions become available.
- **FR-012**: Layout updates caused by newly discovered page sizes MUST avoid large jumps away from the reader's current visual position.
- **FR-013**: Corrupt or unsupported near-visible pages MUST show recoverable failure placeholders and must not prevent surrounding pages from loading.
- **FR-014**: Switching between paged and continuous layout MUST preserve a sensible reading location based on the current or nearest visible page.
- **FR-015**: Two-page spread behavior MUST remain available in paged layout; when continuous vertical layout is active, it MUST not create conflicting side-by-side page placement.
- **FR-016**: Continuous vertical reading MUST preserve existing archive compatibility expectations for readable comic formats.
- **FR-017**: Continuous vertical reading MUST preserve bounded memory behavior so large archives do not require every page texture to be resident at once.

### Key Entities

- **Continuous Reading Layout**: The active vertical reading flow, including total height, visible range, page spacing, and current scroll position.
- **Page Measurement**: The best-known dimensions for a page, either actual dimensions from prepared image data or placeholder dimensions derived from a known aspect ratio.
- **Visible Page Window**: The set of pages whose vertical positions intersect the viewport plus the one-page overdraw margin.
- **Page Display Entry**: A prepared display resource and its measured dimensions that can be reused when a page becomes visible.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Readers can enable continuous vertical layout and scroll from page 1 to page 5 in a 10-page comic without pressing next or previous.
- **SC-002**: In a 100-page comic, the reader prepares no more than the visible pages plus one page above and one page below during steady-state scrolling.
- **SC-003**: During rapid scrolling through a large comic, visible interaction remains responsive with no noticeable input freeze longer than 100 ms.
- **SC-004**: Unknown page sizes reserve stable placeholder space immediately, so the continuous view never collapses to an empty or near-zero-height document while pages load.
- **SC-005**: When actual page dimensions arrive, the reader keeps the current reading location within one visible page of the pre-update location.
- **SC-006**: A corrupt page near the viewport displays a recoverable placeholder while adjacent valid pages continue to appear.

## Assumptions

- Continuous vertical layout is a reader mode alongside existing paged reading, not a replacement for paged reading.
- The default placeholder aspect ratio should be portrait-oriented when no measured page is available.
- Two-page spread mode is treated as a paged-layout feature and is not applied while continuous vertical layout is active.
- Prepared page display resources remain bounded; continuous reading should not keep the full archive resident for display at once.
- Existing page preparation and failure handling behavior remains the basis for loading and displaying individual pages.
