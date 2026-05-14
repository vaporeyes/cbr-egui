# Contract: Spread Viewer

## Purpose

Define the reader-facing behavior for optional two-page spread mode while
preserving the existing viewer/decode/cache boundaries.

## Inputs

- Current comic identity.
- Current page index and page count.
- Spread mode enabled flag.
- Current page display resource or recoverable page status.
- Current page dimensions.
- Optional next page display resource or recoverable page status.
- Next page dimensions when available.

## Outputs

- A display decision: single page, current-plus-next spread, pending side, or
  failed side.
- Updated viewer layout state.
- Optional page-resource requests for the next page when spread mode requires
  it.

## Required Behavior

1. If spread mode is disabled, output a single-page display decision.
2. If spread mode is enabled and the current page width is greater than height,
   output a single-page display decision for a pre-stitched spread.
3. If spread mode is enabled and the current page width is less than or equal to
   height, request the next page when it exists.
4. If the requested next page is ready, output a side-by-side spread decision.
5. If the requested next page is loading or failed, keep the current page
   readable and output the corresponding recoverable side status.
6. If no next page exists, output a single-page display decision without error.
7. Page identity or spread composition changes reset zoom and pan.
8. Scroll and drag interaction update only small viewer state and never perform
   archive reads, image decoding, image resizing, database scans, or texture
   upload.

## Failure Behavior

- Missing current page: show the existing loading/error page state.
- Missing next page in spread mode: render current page with a recoverable
  missing-side state.
- Stale worker result: ignore when generation/page identity no longer matches.
- Invalid dimensions: fall back to single-page recoverable presentation.

## Test Expectations

- Landscape current pages render alone when spread mode is enabled.
- Portrait and square current pages pair with the next page when ready.
- Last page renders alone when no next page exists.
- Page/spread changes reset zoom and pan.
- Stale next-page results do not affect a newer current page.
