# Data Model: Library Spread UI

## AppState

Represents the top-level user-facing mode.

**Variants**

- `Library`: User is browsing the collection grid.
- `Reading`: User is reading one selected comic.

**Relationships**

- Owns or references `LibraryViewState` while browsing.
- Owns or references a `ReadingSession` while reading.

**Validation**

- Selecting a comic transitions from `Library` to `Reading`.
- Returning from the reader transitions back to `Library` without discarding
  collection data.

## LibraryRoot

Represents the configured folder that defines the collection.

**Fields**

- `path`
- `watch_status`
- `last_scan_started_at`
- `last_scan_completed_at`
- `last_error`

**Relationships**

- Drives scanner and watcher workers.
- Produces `ComicRecord` updates through LibraryService.

**Validation**

- Path must exist and be readable to enter active watching.
- Missing or inaccessible paths produce recoverable status instead of panics.

## ComicRecord

Represents one archive in the collection.

**Fields**

- `id`
- `path`
- `hash` or source identity
- `page_count`
- `availability`
- `metadata_id`
- `thumbnail_key`

**Relationships**

- Stored by LibraryService.
- Displayed by `LibraryGridItem`.
- Opened by `ReadingSession`.

**Validation**

- Path is unique.
- Records are updated rather than duplicated when the same path changes.
- Removed archives are deleted or marked unavailable according to storage
  policy chosen during implementation.

## WatchEventBatch

Represents coalesced filesystem changes waiting for reconciliation.

**Fields**

- `root_path`
- `changed_paths`
- `received_at`
- `settle_deadline`

**Relationships**

- Created by watcher worker.
- Consumed by scanner/reconciliation logic.

**Validation**

- Multiple events for the same path within the settle window collapse into one
  reconciliation request.
- Final database state comes from scanning stable filesystem contents.

## CoverThumbnail

Represents a cached cover image derived from the first usable page of a comic.

**Fields**

- `comic_id`
- `source_path`
- `source_fingerprint`
- `cache_path`
- `pixel_size`
- `status`
- `last_error`

**Relationships**

- Generated from archive content via VFS and decode/resize workers.
- Displayed in the library grid after texture upload.

**Validation**

- Display height must not exceed 300px.
- Cache entry is valid only when the source fingerprint matches the current
  archive.
- Missing/corrupt covers produce a recoverable placeholder state.

## LibraryGridItem

Represents the lightweight UI model for one comic tile.

**Fields**

- `comic_id`
- `title`
- `path`
- `thumbnail_status`
- `availability`

**Relationships**

- Derived from `ComicRecord` and `CoverThumbnail`.
- Selection creates a `ReadingSession`.

**Validation**

- Tile layout must remain stable while thumbnails load or fail.
- Unavailable comics cannot start a normal reading session without a
  recoverable message.

## ReadingSession

Represents active reading state for one comic.

**Fields**

- `comic_id`
- `current_page_index`
- `page_count`
- `spread_mode_enabled`
- `spread_decision`
- `viewer_state`

**Relationships**

- Uses library comic facts and page resources prepared through VFS/decode/cache.
- Owns reading interaction state.

**Validation**

- Page changes reset zoom/pan.
- Spread toggle recomputes display decision for the current page.
- Last-page and missing-next-page cases keep current page readable.

## SpreadDecision

Represents how the current page should be displayed.

**Variants**

- `SinglePreStitched`: Current page is wider than tall and renders alone.
- `SingleNoNext`: Current page is eligible for pairing but no next page exists.
- `Pair`: Current page renders side-by-side with the next page.
- `PairPending`: Current page renders while next page is loading.
- `PairFailed`: Current page renders with a recoverable missing-side state.

**Validation**

- Width greater than height always selects `SinglePreStitched`.
- Width less than or equal to height selects `Pair` only when a next page exists
  and is displayable.
- Decision changes are generation-checked so stale worker results do not alter a
  newer page or spread.

## State Transitions

```text
Library -> Reading(comic_id)       when user opens a comic tile
Reading(comic_id) -> Library       when user returns to library
Reading single -> Reading spread   when spread toggle is enabled
Reading spread -> Reading single   when spread toggle is disabled
Thumbnail missing -> loading       when cover generation is queued
Thumbnail loading -> ready         when cached image is prepared
Thumbnail loading -> failed        when archive/page preparation fails
Watch idle -> pending scan         when filesystem events arrive
Pending scan -> synchronized       when stable scan updates finish
```
