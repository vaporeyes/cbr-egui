# Feature Specification: Prefetch & VRAM Cache Integration

**Feature Branch**: `005-prefetch-vram-cache`  
**Created**: 2026-05-13  
**Status**: Draft  
**Input**: User description: "Phase 5: Prefetch & VRAM Cache Integration. The prefetch math and decode workers exist, but the application loop does not actively dispatch or reconcile prefetch requests. Implement dispatcher, reconcile background decodes into texture cache, and cancel stale in-flight prefetch requests."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Nearby Pages Open Instantly (Priority: P1)

As a reader, I want nearby pages to be prepared while I am viewing the current page so that turning to the next or recently previous page feels immediate.

**Why this priority**: Smooth page turns are the core reading experience. Existing prefetch planning has no user value until it actively prepares pages in the app loop.

**Independent Test**: Open a comic, view a page, wait briefly, then navigate to the next page. The next page should appear from prepared content without a visible full decode delay.

**Acceptance Scenarios**:

1. **Given** a readable comic is open on page 10, **When** the reader remains on page 10 after the page loads, **Then** pages 11, 12, and 9 are queued for background preparation if they are not already cached or in progress.
2. **Given** page 11 has completed background preparation, **When** the reader advances from page 10 to page 11, **Then** the prepared page is displayed from cache without repeating the heavy decode path.

---

### User Story 2 - Background Results Become Displayable Cache Entries (Priority: P2)

As a reader, I want background page preparation to become real display-ready content so that prefetched work is not wasted.

**Why this priority**: A dispatcher alone does not improve the app unless completed background work is reconciled into the display cache.

**Independent Test**: Trigger prefetch from an open comic and verify that completed background pages are represented as reusable cached display entries.

**Acceptance Scenarios**:

1. **Given** a prefetch request completes successfully, **When** the application processes background results, **Then** the page is converted into a display-ready cached page.
2. **Given** a prefetch request fails because the page is corrupt or unsupported, **When** the application processes background results, **Then** the failure is recorded or ignored without blocking the visible page or freezing the interface.

---

### User Story 3 - Stale Work Is Cancelled During Fast Navigation (Priority: P3)

As a reader, I want rapid jumps through a comic to prioritize where I actually land, so that old background work does not delay the page I want.

**Why this priority**: Without stale-request cancellation, rapid navigation can flood the worker queue and make the app feel stuck behind irrelevant work.

**Independent Test**: Navigate quickly from an early page to a much later page and confirm that old in-progress prefetches are cancelled and replaced by requests near the new page.

**Acceptance Scenarios**:

1. **Given** prefetch requests are in progress around page 2, **When** the reader jumps to page 50, **Then** requests that are no longer near the active page are cancelled and removed from the active prefetch set.
2. **Given** cancelled background work returns after cancellation, **When** the application processes the result, **Then** it is ignored and does not replace useful cached content.

### Edge Cases

- The current page is at the first or last page and some preferred neighbors are out of range.
- The reader turns pages faster than background preparation can complete.
- A page is already cached, queued, or in progress when the dispatcher evaluates prefetch candidates.
- A corrupt image, unsupported page, or archive read failure occurs during background preparation.
- A cancelled background request returns a late result after the user has moved to a different page.
- The cache reaches capacity while background results are being inserted.
- The application is closed while background work is queued or in progress.
- The visible page must remain responsive while archive reading, image decoding, cache insertion, and stale-request cancellation occur.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The reader MUST evaluate nearby-page preparation whenever the active page changes and after the active page becomes visible.
- **FR-002**: The reader MUST request preparation for the next page, following page, and previous page when those pages exist and are not already display-ready, queued, or in progress.
- **FR-003**: The reader MUST track queued and in-progress background page requests separately from display-ready cached pages.
- **FR-004**: Each background page request MUST carry a cancellation handle that can be activated when the request is no longer useful.
- **FR-005**: When the active page changes, the reader MUST cancel in-progress background requests that are no longer candidates for the new active page.
- **FR-006**: Completed background page preparation MUST be reconciled into the display cache so the page can be shown without repeated heavy preparation.
- **FR-007**: Failed or cancelled background preparation MUST NOT block the visible page, prevent further navigation, or show an intrusive error unless the user navigates directly to the affected page.
- **FR-008**: The display cache MUST keep bounded ownership of prepared pages and evict old entries when capacity is exceeded.
- **FR-009**: Cache eviction MUST release display resources associated with evicted pages.
- **FR-010**: Background preparation MUST never overwrite a newer cache entry for the same page with stale content.
- **FR-011**: The application MUST continue to support single-page and two-page reading while background preparation is active.
- **FR-012**: Reader features MUST preserve the existing separation between library state, reader state, archive access, background preparation, and display rendering.

### Key Entities

- **Prefetch Candidate**: A page near the active page that is useful to prepare before the user asks to view it.
- **Background Page Request**: A pending preparation job for a specific comic page, including its purpose, identity, and cancellation handle.
- **In-Flight Prefetch Set**: The active collection of background requests that have been submitted but have not yet produced a reconciled result.
- **Prepared Page Result**: The outcome of background preparation, either display-ready page content or a recoverable failure.
- **Display Cache Entry**: A reusable page resource held by the reader cache and bounded by cache capacity.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After the current page is visible for 500 milliseconds, at least the next page is prepared or actively being prepared for comics with an available next page.
- **SC-002**: Navigating to a successfully prepared adjacent page displays it within 100 milliseconds on a typical local archive.
- **SC-003**: After jumping more than 10 pages, obsolete background requests are cancelled before new distant-page preparation is scheduled.
- **SC-004**: During rapid navigation across 50 pages, the number of active background page requests remains bounded by the configured nearby-page window plus active visible pages.
- **SC-005**: Cache growth remains bounded during a 30-minute reading session and old prepared pages are evicted without visible rendering errors.
- **SC-006**: The reader remains responsive during page navigation with no noticeable interface freeze while background page preparation, cancellation, and cache reconciliation occur.

## Assumptions

- The prefetch window remains focused on pages `[current + 1, current + 2, current - 1]` unless a future reader mode defines a different strategy.
- Background preparation is best-effort: failures are recoverable and should not interrupt the current visible page.
- Display-ready cached pages are scoped to the currently open comic.
- Two-page spread mode may require the active page and its paired page to be loaded immediately, while prefetch continues to prepare nearby non-visible pages.
- Cache capacity is intentionally bounded and may evict older prepared pages during long reading sessions.
