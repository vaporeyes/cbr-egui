# Feature Specification: egui Viewer Implementation

**Feature Branch**: `003-egui-viewer`  
**Created**: 2026-05-13  
**Status**: Draft  
**Input**: User description: "`egui` Viewer Implementation. Construct the
reading interface. Ensure it is sleek and modern. Think outside the box
regarding UI/UX. Task 3.1: Single Page Viewer. Implement a basic
`egui::CentralPanel` that takes the current `TextureHandle` and renders it using
`ui.image()`. Calculate aspect ratio to fit/fill the window dynamically upon
resize. Task 3.2: Zoom & Pan Mechanics. Wrap the image in an `egui::ScrollArea`.
Intercept scroll wheel and drag events to scale the image dimensions and adjust
the scroll offset. Edge Case: Ensure resetting zoom level triggers when turning
the page."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Read a Single Page Comfortably (Priority: P1)

As a reader opening a comic page, I need the current page to appear centered,
sharp, and proportionally scaled to the available reading area so I can begin
reading without manual adjustment.

**Why this priority**: A usable single-page reading surface is the minimum
viewer experience; zoom, pan, and visual polish all depend on this base view.

**Independent Test**: Can be tested by providing a prepared page texture and
resizing the reading window, then confirming the page remains visible,
proportionally scaled, and centered without distorting the image.

**Acceptance Scenarios**:

1. **Given** a current page is available, **When** the reader view is opened,
   **Then** the page is rendered in the primary reading area.
2. **Given** the window is resized, **When** the reading area changes size,
   **Then** the page is re-fit while preserving its aspect ratio.
3. **Given** the page aspect ratio differs from the window aspect ratio,
   **When** the page is displayed, **Then** no stretching, clipping, or
   unintended distortion occurs in the default view.

---

### User Story 2 - Zoom and Pan Around Page Details (Priority: P2)

As a reader inspecting text or art details, I want fluid zoom and pan controls
so I can focus on small regions of a page without losing my place.

**Why this priority**: Comic pages often contain dense text and detailed art.
The viewer must support close reading after the base page view works.

**Independent Test**: Can be tested by applying scroll and drag interactions to
one displayed page and confirming the page scale and viewport position update
predictably within safe bounds.

**Acceptance Scenarios**:

1. **Given** a page is displayed, **When** the reader scrolls over the page,
   **Then** the page zoom level changes smoothly within configured limits.
2. **Given** the page is zoomed beyond fit level, **When** the reader drags the
   view, **Then** the visible region pans without moving outside the page
   bounds.
3. **Given** the reader alternates zooming and panning, **When** interactions
   are repeated, **Then** the view remains stable and does not jump
   unexpectedly.

---

### User Story 3 - Preserve Modern Reading Flow Across Page Changes (Priority: P3)

As a reader moving through pages, I want page transitions to feel clean and
intentional, with zoom reset appropriately so each new page starts in a readable
default state.

**Why this priority**: Carrying an old zoom/pan state onto a different page is
disorienting. Page-change behavior must reinforce a polished reading flow.

**Independent Test**: Can be tested by zooming and panning one page, switching
to another page, and confirming the next page returns to the default fit view
while keeping the interface responsive.

**Acceptance Scenarios**:

1. **Given** the current page is zoomed or panned, **When** the reader turns to a
   different page, **Then** zoom and pan reset to the default fit view.
2. **Given** the next page has a different aspect ratio, **When** the page
   changes, **Then** the new page is fit according to its own dimensions.
3. **Given** a page change occurs while input is active, **When** the new page
   appears, **Then** stale interaction state does not affect the new page.

### Edge Cases

- No current page texture is available yet because decode or texture upload is
  still pending.
- The current page texture fails or becomes unavailable while visible.
- The window is very small, very wide, very tall, or resized continuously.
- Pages have extreme aspect ratios, including double-page spreads and tall
  webtoon-like pages.
- The reader zooms rapidly with repeated scroll events.
- The reader drags while not zoomed enough to require panning.
- The reader changes pages while zooming or dragging.
- Zoom and pan must not trigger image decoding, archive reads, or other heavy
  work on the reading interaction loop.
- Visual controls or overlays must not obscure the page content in normal
  reading mode.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST display the current prepared page in the primary
  reading area.
- **FR-002**: System MUST preserve page aspect ratio in the default display
  state.
- **FR-003**: System MUST dynamically recalculate the default fit size when the
  reading area is resized.
- **FR-004**: System MUST provide a default fit mode that keeps the full page
  visible without distortion.
- **FR-005**: System MUST support a fill or immersive viewing mode that can use
  more of the window while preserving aspect ratio.
- **FR-006**: System MUST support smooth zooming from reader input while keeping
  zoom within configured minimum and maximum limits.
- **FR-007**: System MUST support panning when the zoomed page is larger than
  the visible reading area.
- **FR-008**: System MUST constrain panning so the viewport does not drift
  beyond useful page bounds.
- **FR-009**: System MUST reset zoom and pan state when the current page changes.
- **FR-010**: System MUST keep zoom, pan, resize, and page-change interactions
  responsive without performing archive access, image decoding, resizing, or
  texture upload during pointer/scroll handling.
- **FR-011**: System MUST preserve the LibraryService and ViewerState boundary
  by keeping library persistence and archive access outside the reading UI
  state.
- **FR-012**: System MUST use prepared display resources from the asynchronous
  image pipeline rather than decoding raw page bytes inside the viewer.
- **FR-013**: System MUST handle missing or unavailable current page resources
  with a recoverable empty/loading/error presentation rather than panicking.
- **FR-014**: System MUST keep visible controls and status affordances minimal
  and non-obstructive during normal reading.
- **FR-015**: System MUST support a polished visual presentation suitable for a
  modern desktop reading app, including calm spacing, restrained chrome, and
  content-first composition.

### Key Entities

- **Viewer State**: Current reading UI state. Key attributes include current
  page identity, view mode, zoom level, pan offset, viewport size, and whether a
  page resource is available.
- **Page Viewport**: The visible reading area available for the page. Key
  attributes include width, height, and resize-driven fit calculations.
- **Page Display Resource**: A prepared page image resource ready for display.
  Key attributes include page identity, pixel dimensions, and display handle.
- **Zoom/Pan State**: Per-page interaction state for scale and visible region.
  Key attributes include current scale, minimum and maximum scale, pan offset,
  and reset generation.
- **Viewer Chrome**: Minimal controls, overlays, or status affordances around the
  reading surface. Key attributes include visibility, placement, and whether
  they obscure page content.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A prepared page can be displayed in the reader view within one
  interaction update after it becomes available.
- **SC-002**: Default fit keeps 100% of the page visible for representative
  portrait, landscape, square, tall, and double-page aspect ratios.
- **SC-003**: Resize handling preserves aspect ratio in 100% of tested viewport
  sizes without visual stretching.
- **SC-004**: Zoom input changes page scale within configured limits in 100% of
  tested scroll interactions.
- **SC-005**: Pan input keeps the visible region within useful page bounds in
  100% of tested drag interactions.
- **SC-006**: Turning the page resets zoom and pan state in 100% of tested page
  changes.
- **SC-007**: The reader view remains responsive during zoom, pan, resize, and
  page-change interactions with no heavy page preparation work performed in the
  interaction path.
- **SC-008**: Missing or unavailable page resources produce a recoverable
  loading or error presentation in 100% of tested cases.

## Assumptions

- Prepared page display resources are supplied by the asynchronous image
  pipeline and its bounded cache.
- This feature covers single-page reading only; two-page spread mode and
  continuous scrolling remain future viewer modes.
- Default zoom reset on page turn is preferred over preserving per-page zoom
  history for this feature.
- Fill mode may crop page edges only when explicitly selected by the reader; the
  default mode keeps the full page visible.
- Visual polish focuses on the reading surface and minimal non-obstructive
  affordances rather than a full library navigation shell.
