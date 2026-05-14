# Contract: Continuous Vertical Reader

This contract defines the behavior expected between reader state, layout geometry, background page preparation, and the rendered continuous canvas.

## Mode Contract

- The reader exposes a paged mode and a continuous vertical mode.
- Entering continuous vertical mode disables side-by-side spread composition for the active view.
- Leaving continuous vertical mode returns to paged rendering at the page nearest the current visible reading location.
- Switching modes must not discard prepared page display entries for the active comic.

## Virtual Canvas Contract

- Given a page count, viewport width, page gap, and page measurements, the reader produces an ordered virtual canvas.
- Every page has one rectangle in the virtual canvas.
- Actual page dimensions are used when known.
- Unknown pages use placeholder dimensions derived from the first known page ratio.
- If no actual page ratio is known, unknown pages use a stable portrait fallback.
- Total height equals the sum of valid page heights plus gaps between pages.
- Recomputing the canvas after resize or new measurements must keep the current reading location anchored to the nearest visible page when possible.

## Visible Window Contract

- The reader determines the viewport rectangle in virtual-canvas coordinates every frame while continuous mode is active.
- Pages whose rectangles intersect the viewport are visible pages.
- The near-visible window includes all visible pages plus at most one nearest page above and one nearest page below.
- Only pages in the near-visible window are eligible for display preparation for continuous rendering.
- Pages already prepared, queued, or in flight are not requested again.
- Requests that fall outside the updated near-visible window after rapid scroll may be cancelled or ignored if they complete late.

## Page Preparation Contract

- Missing near-visible pages are prepared through the existing background page pipeline.
- The render loop may check cache state, compute geometry, poll completed results non-blockingly, and upload completed display resources.
- The render loop must not perform archive reads, decompression, image decoding, PDF rendering, image resizing, or library database scans.
- Successful preparation records actual page dimensions and inserts a bounded display entry.
- Failed preparation records a recoverable page failure and reserves placeholder space for the failed page.

## Rendering Contract

- Prepared pages inside the visible viewport are painted at their virtual-canvas rectangles.
- Near-visible overdraw pages may be prepared but do not need to be painted until they intersect the viewport.
- Missing pages inside the viewport draw stable loading placeholders.
- Failed pages inside the viewport draw recoverable failure placeholders.
- Distant pages outside the near-visible window are not painted and are not requested solely because continuous mode is active.

## Memory Contract

- Display resources remain bounded by the existing active-reader cache limit.
- Continuous mode may retain lightweight page measurements for the active comic.
- Continuous mode must not keep every page texture resident for large archives.
