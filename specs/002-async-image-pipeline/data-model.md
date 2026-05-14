# Data Model: Asynchronous Image Pipeline

## DecodeRequest

Represents one request to prepare a page image.

**Fields**

- `request_id`: Unique request identity for stale result detection.
- `page_index`: Zero-based page index in the currently opened comic.
- `bytes`: Raw page bytes supplied by the archive VFS.
- `purpose`: Direct view request or prefetch request.
- `target_size`: Optional display bounds used for pre-upload downsampling.

**Relationships**

- Produced by ViewerState or page-loading orchestration after VFS byte read.
- Consumed by the worker pool.
- Produces one DecodeResult.

**Validation**

- `page_index` must be within the current comic page range before request
  submission.
- `bytes` must be non-empty.
- Duplicate prefetch requests are skipped when page index is cached, queued, or
  in-flight.

**State Transitions**

- Created -> queued.
- Queued -> in-flight when a worker starts it.
- In-flight -> succeeded, failed, or ignored during shutdown.

## DecodeResult

Represents the output of one decode request.

**Fields**

- `request_id`: Original request identity.
- `page_index`: Page index associated with the request.
- `outcome`: Decoded page image or recoverable decode error.

**Relationships**

- Returned from worker pool to main thread.
- Successful results are eligible for texture promotion and cache insertion.
- Failed results are retained only long enough for caller recovery.

**Validation**

- Result identity must match a known in-flight request before promotion.
- Stale results may be ignored when no longer relevant to the active comic or
  page generation.

## WorkerPool

Represents background decode capacity.

**Fields**

- `worker_count`: Number of decode worker threads.
- `request_sender`: Queue endpoint for decode requests.
- `result_receiver`: Main-thread endpoint for decode results.
- `in_flight`: Page/request identities currently being processed.
- `shutdown_state`: Whether workers are accepting new work.

**Relationships**

- Owns worker threads.
- Receives DecodeRequest values and emits DecodeResult values.

**Validation**

- Worker count must be at least one.
- Shutdown must stop accepting new work and allow thread cleanup.

## PrefetchWindow

Represents ordered page candidates selected from current-page state.

**Fields**

- `current_page`: Current page index.
- `page_count`: Total pages in the current comic.
- `candidates`: Ordered page indices, normally `n+1`, `n+2`, `n-1` after range
  and duplicate filtering.

**Relationships**

- Reads ViewerState page position and cache/queue state.
- Produces prefetch DecodeRequest values.

**Validation**

- Candidate indices must be within `0..page_count`.
- Cached, queued, and in-flight candidates are skipped.

## DisplayCacheEntry

Represents a display resource retained for reuse.

**Fields**

- `page_index`: Page index.
- `texture`: Main-thread display texture handle.
- `last_used`: LRU ordering metadata.

**Relationships**

- Owned by ViewerState.
- Created from successful DecodeResult values on the main thread.

**Validation**

- Cache capacity defaults to 5 and must not exceed 10 for this feature.
- Inserting beyond capacity evicts the least-recently-used entry.
