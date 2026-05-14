# Quickstart: Prefetch & VRAM Cache Integration

## Prerequisites

- A readable comic archive with at least 5 pages.
- At least one archive with a corrupt or unsupported page for failure-path validation.

## Automated Checks

Run formatting and tests:

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
cargo test --test prefetch_scheduler
cargo test --test page_cache
cargo test --test decode_pipeline
cargo test --test app_routing
```

Expected coverage:

- Candidate math excludes cached, queued, in-flight, and out-of-range pages.
- Background decode requests can be cancelled.
- Texture cache remains bounded and evicts least-recently-used pages.
- Reader navigation checks cached pages before loading directly.
- Late stale results do not overwrite current cache entries.

## Implementation Notes

- Use multi-page CBZ fixtures in `tests/app_routing.rs` for deterministic prefetch and navigation checks.
- Treat `.specify/feature.json` as the feature-directory source of truth until the repository has an initial commit and Spec Kit branch checks can resolve `HEAD`.
- Validate prefetch behavior in small increments: foundation state, dispatch, reconciliation, cancellation, then manual UX checks.
- Current implementation dispatches `[n+1, n+2, n-1]` after the visible page is ready, drains decode results non-blockingly in the app update loop, uploads completed prefetches on the egui thread, and reuses cached textures before falling back to direct archive reads.
- Rapid page jumps cancel in-flight pages outside the new nearby-page window; late cancelled or old-generation results are ignored after being removed from tracking.
- Direct visible-page loads still read/decode synchronously as a fallback path. Prefetch keeps adjacent navigation fast, but a cache miss on the actively requested page can still pay archive read/decode cost.

## Manual Reader Validation

1. Start the app.
2. Open a library folder with a multi-page comic.
3. Open a comic and wait briefly on page 1.
4. Navigate to page 2.
5. Confirm page 2 appears quickly and page turns remain responsive.
6. Rapidly jump or repeatedly press next across many pages.
7. Confirm the UI remains responsive and no old pages appear after the jump.
8. Enable two-page mode.
9. Confirm the visible paired page still loads while nearby non-visible pages continue to prepare.
10. Open or navigate to a comic with a corrupt page.
11. Confirm the current page does not freeze and direct navigation to the corrupt page shows a recoverable failure.

## Performance Expectations

- Adjacent prepared page turn: under 100 ms on a typical local archive.
- Active background request set: bounded by the nearby-page window plus visible spread needs.
- Cache size: never exceeds configured capacity.
- No long-running archive read, image decode, PDF render, or resize work should run directly in the egui render loop.
