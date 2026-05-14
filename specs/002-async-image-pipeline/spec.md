# Feature Specification: Asynchronous Image Pipeline

**Feature Branch**: `002-async-image-pipeline`  
**Created**: 2026-05-13  
**Status**: Draft  
**Input**: User description: "The Asynchronous Image Pipeline. Isolate all
heavy lifting from the egui context. Implement the Worker Pool. Spawn a
dedicated thread pool for decoding. Build the Decode Pipeline. Create a pipeline
that accepts raw bytes, decodes via image::load_from_memory, and outputs an
egui::ColorImage. Implement Smart Pre-fetching. Monitor the current_page index.
Push indices [n+1, n+2, n-1] to the worker channel. When the worker returns an
egui::ColorImage, the main thread converts it to an egui::TextureHandle and
inserts it into the LRU cache."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Decode Pages Away From Reading Interaction (Priority: P1)

As a reader navigating pages, I need image decoding and resizing work isolated
from the reading interaction loop so page changes, zooming, and input remain
responsive while pages are prepared in the background.

**Why this priority**: Responsiveness is the core value of this feature; without
background decoding, later cache and prefetch behavior cannot protect the
reading experience.

**Independent Test**: Can be tested by submitting raw page bytes for decoding
while simulating reading interaction and confirming decoded page images are
returned asynchronously without blocking navigation state updates.

**Acceptance Scenarios**:

1. **Given** valid raw image bytes, **When** a decode request is submitted,
   **Then** the work is processed outside the reading interaction loop and a
   display-ready page image is returned to the main thread.
2. **Given** multiple pages queued for decode, **When** workers are busy,
   **Then** the reader can continue updating current-page state while decode
   results arrive later.
3. **Given** invalid or corrupt image bytes, **When** decoding is attempted,
   **Then** the caller receives a recoverable page-decode failure and reading
   interaction can continue.

---

### User Story 2 - Prefetch Nearby Pages (Priority: P2)

As a reader moving through a comic, I want nearby pages prepared before I ask
for them so normal forward and short backward navigation feels immediate.

**Why this priority**: Prefetching turns the background pipeline into visible
reader value by preparing likely next pages rather than only reacting after a
page is requested.

**Independent Test**: Can be tested by setting a current page and confirming the
pipeline schedules nearby page indices in the expected priority order while
avoiding out-of-range and duplicate work.

**Acceptance Scenarios**:

1. **Given** the current page is `n`, **When** prefetching is evaluated, **Then**
   the system schedules `n+1`, `n+2`, and `n-1` when those pages exist.
2. **Given** the current page is at the start or end of a comic, **When**
   prefetching is evaluated, **Then** page indices outside the valid page range
   are not scheduled.
3. **Given** a nearby page is already decoded, cached, or currently queued,
   **When** prefetching is evaluated, **Then** duplicate decode work is not
   scheduled for that page.

---

### User Story 3 - Promote Finished Pages Into the Display Cache (Priority: P3)

As a reader viewing decoded pages, I need completed page images promoted into a
bounded display cache on the main thread so the reader can reuse prepared pages
without unbounded memory growth.

**Why this priority**: Decoding alone is not sufficient; completed pages must be
handed back safely for display and retained only within controlled memory
limits.

**Independent Test**: Can be tested by returning decoded page results to the
main thread, converting them into display resources, and confirming cache
insertion, reuse, and eviction respect the configured limit.

**Acceptance Scenarios**:

1. **Given** a worker finishes decoding a page, **When** the main thread
   receives the result, **Then** the decoded image is converted into a display
   resource on the main thread.
2. **Given** the display cache is below capacity, **When** a decoded page is
   promoted, **Then** the display resource is inserted and can be reused for
   subsequent navigation.
3. **Given** the display cache is at capacity, **When** another decoded page is
   promoted, **Then** the least-recently-used entry is evicted before the new
   page is retained.

### Edge Cases

- Decode requests arrive faster than workers can complete them.
- A page is requested directly while the same page is already queued for
  prefetch.
- Current page changes rapidly before earlier prefetch results return.
- The current page is the first page, last page, or the comic has fewer than
  three pages.
- Raw bytes are corrupt, truncated, unsupported, or decode into an image too
  large to keep unbounded in memory.
- A worker fails while processing a decode request.
- A decoded result arrives for a page that is no longer near the current page.
- Cache capacity is reached while new decoded pages continue arriving.
- Main-thread display-resource creation fails or the display context is not
  ready to accept a result.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a dedicated background worker capability for
  page image decoding work.
- **FR-002**: System MUST accept raw page bytes as decode input and return a
  display-ready decoded page image result to the main thread.
- **FR-003**: System MUST ensure image decoding, page resizing, and other
  CPU-heavy page preparation do not run in the reading interaction loop.
- **FR-004**: System MUST report corrupt, unsupported, or failed image decodes
  as recoverable page errors rather than stopping reading interaction.
- **FR-005**: System MUST allow multiple decode requests to be queued while
  preserving enough request identity to associate each result with its page.
- **FR-006**: System MUST monitor current-page changes and schedule nearby page
  prefetch candidates in the priority order next page, following page, previous
  page.
- **FR-007**: System MUST skip prefetch candidates that are outside the valid
  page range.
- **FR-008**: System MUST avoid duplicate decode work for pages that are already
  cached, currently queued, or currently being decoded.
- **FR-009**: System MUST return completed decoded images to the main thread for
  display-resource creation.
- **FR-010**: System MUST insert created display resources into a bounded
  least-recently-used cache.
- **FR-011**: System MUST evict least-recently-used display resources when cache
  capacity is reached.
- **FR-012**: System MUST preserve the LibraryService and ViewerState boundary
  by keeping archive/library access separate from reading display state.
- **FR-013**: System MUST use the archive VFS as the source of page bytes rather
  than reading archive payloads through an alternate path.
- **FR-014**: System MUST keep the configured cache capacity at or below 10 full
  pages unless a later plan explicitly justifies a higher limit.
- **FR-015**: System MUST allow stale decode or prefetch results to be ignored
  safely when the reader has moved elsewhere.

### Key Entities

- **Decode Request**: A request to prepare one page image. Key attributes
  include page index, raw page bytes, and request purpose such as direct view or
  prefetch.
- **Decode Result**: The outcome of a decode request. Key attributes include
  page index, decoded image payload, or recoverable failure details.
- **Worker Pool**: Background capacity that processes decode requests without
  blocking reading interaction.
- **Prefetch Window**: The ordered set of nearby page indices selected from the
  current page.
- **Display Cache Entry**: A main-thread display resource associated with a page
  index and retained under least-recently-used eviction.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: During a test with at least 25 queued page decode requests,
  reading interaction state can continue accepting page changes without waiting
  for any individual decode to complete.
- **SC-002**: Valid page bytes produce a display-ready decoded page result in at
  least 95% of tested supported image samples.
- **SC-003**: Corrupt or unsupported image bytes produce recoverable failures in
  100% of tested cases without stopping later decode requests.
- **SC-004**: For a middle page in a comic with at least four pages, prefetch
  scheduling selects the next page, following page, and previous page in that
  order.
- **SC-005**: For first-page and last-page navigation, prefetch scheduling
  avoids out-of-range page indices in 100% of tested cases.
- **SC-006**: Duplicate prefetch scheduling is avoided for cached, queued, or
  in-progress pages in 100% of tested duplicate scenarios.
- **SC-007**: Display cache size never exceeds the configured page capacity
  during tests that return more decoded pages than the cache can hold.
- **SC-008**: Page preparation work remains outside the reading interaction loop
  for all tested decode and prefetch paths.

## Assumptions

- This feature builds on the existing archive VFS data layer for retrieving raw
  page bytes.
- The main thread is responsible for creating display resources from decoded
  image results.
- A default prefetch window of next page, following page, and previous page is
  sufficient for this feature.
- A bounded cache of 5 to 10 full pages is sufficient for this feature unless a
  later feature justifies a different limit.
- Network sync, PDF rendering, and persistent cache storage are outside this
  feature.
