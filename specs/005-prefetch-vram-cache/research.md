# Research: Prefetch & VRAM Cache Integration

## Decision: Scope Prefetch State to the Active Reading Session

**Rationale**: Prefetch candidates, in-flight requests, and cached textures are only valid for the currently open comic. Keeping this state in the reading session prevents cached page indices from one comic being reused for another and preserves the LibraryService/ViewerState boundary.

**Alternatives considered**:

- Global app cache keyed by comic and page: more reusable, but increases invalidation and VRAM lifetime complexity before the app has persistent multi-comic reader tabs.
- ViewerState-owned prefetch state: rejected because ViewerState should stay focused on display and interaction state, not archive/decode coordination.

## Decision: Reuse the Existing Decode Worker Pool

**Rationale**: The project already has bounded worker threads, request IDs, decode purposes, and cancellation tokens. Reusing this path avoids introducing another concurrency model and aligns with the constitution's `std::thread` plus channel requirement.

**Alternatives considered**:

- Create a separate prefetch worker pool: useful for priority isolation later, but unnecessary for the current small prefetch window.
- Decode synchronously during navigation: rejected because reader decode work must not block the interface.

## Decision: Main Thread Reconciles Color Images into Texture Cache

**Rationale**: Background workers can decode image bytes into CPU-side color images, but display textures require the UI context. Polling worker results during the app update loop lets successful prefetches become cached display resources without moving UI ownership into worker threads.

**Alternatives considered**:

- Store only decoded CPU images and upload on page turn: reduces VRAM use but fails the "display-ready cache entry" requirement and can still cause page-turn latency.
- Upload textures from background threads: rejected because texture handles are UI/display resources.

## Decision: Cancel Stale Requests on Active Page Change

**Rationale**: Rapid jumps can make previous neighbor requests irrelevant. Cancelling in-flight stale candidates and removing them from tracking ensures the worker queue prioritizes the user's current location.

**Alternatives considered**:

- Let all queued work finish: simple, but stale work can delay useful pages after fast navigation.
- Clear the whole worker pool: too disruptive; direct visible page work and still-relevant neighbor work should remain valid.

## Decision: Use Existing Bounded LRU Cache Limits

**Rationale**: The current page cache has a default capacity of 5 and a hard max of 10 pages, matching the constitution. Phase 5 should integrate with this cache rather than define a second unbounded texture store.

**Alternatives considered**:

- Separate prefetch-only cache: increases eviction complexity and can duplicate textures.
- Unbounded cache during a reading session: rejected because high-resolution pages can exhaust RAM/VRAM.

## Decision: Failed Prefetches Are Non-Intrusive

**Rationale**: A failed background decode should not interrupt the current visible page. The user should see an error page only when directly navigating to the failed page, consistent with existing recoverable failure behavior.

**Alternatives considered**:

- Surface all prefetch failures in reader chrome: noisy and not actionable while the user is viewing a different page.
- Retry failed prefetches indefinitely: risks repeated work for corrupt pages.
