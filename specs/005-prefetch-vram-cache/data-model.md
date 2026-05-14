# Data Model: Prefetch & VRAM Cache Integration

## ReadingSession

Represents the active comic reading session.

**Fields affected by this feature**:

- `comic_id`: active comic identity.
- `current_page_index`: active visible page.
- `page_count`: total readable pages.
- `viewer_state`: current display state.
- `prefetch`: session-scoped background preparation state.
- `texture_cache`: bounded display-ready page cache for the active comic.
- `decode_worker_pool`: bounded worker pool used for page decode requests.

**Relationships**:

- Owns one `PrefetchRuntime`.
- Owns one `PageTextureCache` for the active comic.
- Feeds ready textures into `ViewerState` for visible pages.

**Validation rules**:

- `current_page_index` must remain `< page_count` when `page_count > 0`.
- Cached page entries must refer only to the active comic.
- Session teardown cancels outstanding prefetch work and drops cached textures.

## PrefetchRuntime

Tracks transient background page preparation for the active reading session.

**Fields**:

- `generation`: increments when active page context changes.
- `queued_pages`: page indices submitted or pending submission.
- `in_flight`: map of page index to `InFlightPrefetch`.
- `failed_pages`: optional recoverable failures for pages encountered during background preparation.
- `next_request_id`: monotonic request identity allocator.

**Relationships**:

- Reads candidate pages from `PrefetchState`.
- Owns cancellation handles for in-flight prefetches.
- Produces `PreparedPageResult` records for reconciliation.

**Validation rules**:

- A page index may appear in at most one of queued, in-flight, or cached sets.
- In-flight requests that are no longer candidates for the active page must be cancelled before new distant candidates are submitted.
- Request IDs must uniquely identify outstanding decode requests within the session.

## InFlightPrefetch

Represents one submitted background page preparation request.

**Fields**:

- `page_index`: page being prepared.
- `request_id`: decode request identity.
- `generation`: active-page generation when submitted.
- `cancellation_token`: handle used to cancel stale work.

**State transitions**:

```text
Queued -> InFlight -> Reconciled
Queued -> Cancelled
InFlight -> Cancelled
InFlight -> Failed
Cancelled -> IgnoredResult
```

**Validation rules**:

- Cancellation is idempotent.
- A cancelled request result must not insert or replace a display cache entry.

## PreparedPageResult

Represents a worker result after background decode.

**Fields**:

- `page_index`: decoded page.
- `request_id`: associated decode request.
- `generation`: request generation used for stale-result detection.
- `outcome`: displayable image data or recoverable error.

**Validation rules**:

- Results must match the current in-flight request before cache insertion.
- Successful results are converted to display textures on the UI thread.
- Failed results remove the page from in-flight tracking and may mark the page as failed for direct navigation handling.

## Display Cache Entry

Represents a display-ready cached page.

**Fields**:

- `page_index`: active-comic page key.
- `texture`: display resource for rendering.
- `pixel_size`: source image dimensions after decode/downsample.

**Validation rules**:

- Cache size is bounded by configured capacity.
- Inserting a new entry may evict least-recently-used entries.
- Evicted display resources must be released by dropping or invoking the configured eviction path.

## Prefetch Candidate

Represents a page index selected by nearby-page strategy.

**Fields**:

- `page_index`: candidate page.
- `priority`: implicit ordering: next page, following page, previous page.

**Validation rules**:

- Candidate must be within `[0, page_count)`.
- Candidate must not already be cached, queued, or in flight.
- Candidate set is recalculated when active page changes.
