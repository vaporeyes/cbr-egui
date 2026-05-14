# Quickstart: Continuous Vertical Scroll

## Prerequisites

- A readable archive with at least 10 pages.
- A larger archive with 100 or more pages for responsiveness checks.
- An archive with one corrupt or unsupported page for failure-path validation.

## Automated Checks

Run formatting and the full test suite:

```sh
cargo fmt
cargo test
```

Run lint checks:

```sh
cargo clippy --all-targets -- -D warnings
```

## Focused Validation

Run focused tests during implementation:

```sh
cargo test --test viewer_layout
cargo test --test viewer_interaction
cargo test --test spread_viewer
cargo test --test app_routing
cargo test --test page_cache
```

Expected coverage:

- Continuous canvas height and page rectangle calculations are stable.
- Viewport intersection returns only visible pages plus one page of overdraw.
- Placeholder page sizes remain positive before actual dimensions are known.
- Actual page sizes replace placeholders without losing reading location.
- Near-visible pages are requested through background preparation and cache checks.
- Distant pages are not requested solely because continuous mode is active.
- Corrupt near-visible pages produce recoverable placeholders.

## Implementation Notes

- Continuous layout geometry lives in `src/viewer/continuous.rs` and is intentionally testable without an egui context.
- Continuous mode disables side-by-side spread composition while active; toggling back to paged mode restores discrete page rendering.
- Near-visible preparation uses the existing decode worker pool and cancellation tokens. The candidate set comes from the visible viewport plus one page of overdraw, not the paged `[n+1, n+2, n-1]` prefetch formula.
- Page measurements are lightweight and can outlive texture cache eviction, so long archives do not need every page texture resident just to preserve scroll height.
- The current implementation preserves scroll location through pure anchor helpers; final manual validation should confirm egui scroll offset restoration feels stable during real window resizing and progressive measurement updates.

## Manual Reader Validation

1. Start the app.
2. Open a library folder with a multi-page comic.
3. Open a comic and enable continuous vertical layout.
4. Scroll from page 1 through at least page 5 without using next or previous buttons.
5. Confirm pages appear in one vertical flow with stable spacing.
6. Scroll rapidly through a large comic.
7. Confirm the UI remains responsive and pages load progressively near the viewport.
8. Resize the window while continuous mode is active.
9. Confirm visible content stays near the same reading location and page dimensions update cleanly.
10. Toggle back to paged mode.
11. Confirm the reader lands on the nearest visible page.
12. Open or navigate to a comic with a corrupt page near the viewport.
13. Confirm the corrupt page shows a recoverable placeholder and adjacent pages remain readable.

## Performance Expectations

- No noticeable input freeze longer than 100 ms during rapid scroll.
- Steady-state page preparation is limited to visible pages plus one page above and below.
- Display cache capacity remains bounded by existing page cache limits.
- Continuous layout stores lightweight measurements, not full textures, for distant pages.
