# Data Model: Continuous Vertical Scroll

## ReadingSession

Represents the active comic reading session.

**Fields affected by this feature**:

- `comic_id`: active comic identity.
- `current_page_index`: page nearest to the current reading location.
- `page_count`: total readable pages.
- `viewer_state`: canvas and reader interaction state.
- `texture_cache`: bounded display-ready page cache for the active comic.
- `decode_worker_pool`: bounded worker pool used for page preparation.
- `continuous_scroll`: transient continuous layout state.

**Relationships**:

- Owns one `ContinuousScrollState` while the comic is open.
- Owns one bounded display cache used by both paged and continuous layouts.
- Feeds visible-window page requests to background preparation.

**Validation rules**:

- `current_page_index` must remain within the active comic page range.
- Continuous state must be cleared or reset when opening a different comic.
- Session teardown cancels outstanding work and drops display resources.

## ContinuousScrollState

Represents transient state for vertical continuous reading.

**Fields**:

- `page_measurements`: per-page known or placeholder dimensions.
- `visible_window`: current near-visible page window.
- `scroll_anchor`: page and intra-page offset used to preserve reading location during recalculation.
- `gap`: vertical spacing between pages.
- `placeholder_ratio`: aspect ratio used when actual page dimensions are unknown.

**Relationships**:

- Uses `PageMeasurement` entries to build a `VirtualCanvas`.
- Produces a `VisiblePageWindow` for display preparation.
- Updates `ViewerState.layout_mode` when continuous reading is active.

**Validation rules**:

- Placeholder dimensions must be positive and finite.
- Known page measurements must replace placeholders without removing the page entry.
- The visible window must stay within `[0, page_count)`.
- Layout recalculation must preserve the scroll anchor when possible.

## PageMeasurement

Represents the best-known dimensions for one page.

**Fields**:

- `page_index`: page identity within the active comic.
- `size`: width and height used for layout.
- `source`: actual decoded size, placeholder from first known ratio, or default placeholder.
- `failure`: optional recoverable failure message for corrupt or unsupported pages.

**State transitions**:

```text
Unknown -> Placeholder
Unknown -> Actual
Placeholder -> Actual
Placeholder -> FailedPlaceholder
Actual -> FailedPlaceholder only if a newer direct load fails for the same page
```

**Validation rules**:

- Page index must be within the active comic page range.
- Size must be positive and finite.
- Actual measurements take precedence over placeholders.
- Failed placeholders still reserve stable layout space.

## VirtualCanvas

Represents the computed vertical document geometry.

**Fields**:

- `total_height`: summed page heights plus gaps.
- `page_rects`: ordered page rectangles within the vertical document.
- `viewport_width`: width used to derive display sizes.

**Relationships**:

- Built from `PageMeasurement` entries.
- Used to determine page/viewport intersections.

**Validation rules**:

- Rectangles must be ordered by page index.
- Rectangles must not overlap except at zero-sized boundaries.
- Total height must match the final page bottom plus gaps.
- Recomputing after resize must produce valid positive rectangles.

## VisiblePageWindow

Represents pages eligible for display preparation in continuous mode.

**Fields**:

- `visible_pages`: pages intersecting the viewport.
- `overdraw_pages`: one nearest page above and one nearest page below when available.
- `viewport_rect`: current visible document rectangle.

**Relationships**:

- Derived from `VirtualCanvas`.
- Feeds display cache checks and background preparation requests.

**Validation rules**:

- Pages outside the visible window plus overdraw must not be requested solely for continuous rendering.
- The same page must not be requested twice if it is already cached, queued, or in flight.
- Window recalculation after scroll must cancel stale near-visible requests that are no longer useful.

## Continuous Page Display Entry

Represents a page ready to draw in the continuous canvas.

**Fields**:

- `page_index`: active-comic page key.
- `texture`: display resource for rendering.
- `pixel_size`: actual image dimensions.
- `layout_rect`: current rectangle in the virtual canvas.

**Validation rules**:

- Display entries must respect the bounded cache capacity.
- Evicted display resources may leave their actual measurements behind for layout stability.
- Failed pages draw recoverable placeholders instead of blocking adjacent pages.
