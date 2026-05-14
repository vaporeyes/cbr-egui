# Quickstart: Asynchronous Image Pipeline

## Prerequisites

- Rust 2024 toolchain available through Cargo.
- Existing archive VFS feature available for page-byte retrieval.

## Validate the Feature

1. Check the crate builds:

   ```bash
   cargo check
   ```

2. Run the focused test suite:

   ```bash
   cargo test
   ```

3. Decode pipeline validation expected from tests:

   - Submit valid in-memory image bytes and receive a decoded page image.
   - Submit corrupt bytes and receive a recoverable decode error.
   - Queue at least 25 decode requests and confirm request identity is preserved
     in returned results.

4. Prefetch validation expected from tests:

   - With current page `n`, candidates are returned as `n+1`, `n+2`, `n-1`.
   - First-page and last-page cases omit out-of-range candidates.
   - Cached, queued, and in-flight candidates are skipped.

5. Cache validation expected from tests:

   - Default capacity is 5 pages.
   - Capacity above 10 is rejected.
   - Inserting more pages than capacity evicts the least-recently-used entry.
   - Accessing a page refreshes its recency.

6. Viewer-boundary validation expected from tests or code inspection:

   - Worker code does not create texture handles.
   - Texture cache insertion happens through the main-thread integration path.
   - Decode/cache/viewer modules do not bypass LibraryService or VFS for archive
     bytes.

## Manual Smoke Check

After a minimal viewer integration exists, open a small archive, navigate
forward several pages, then navigate backward once. Page interaction should stay
responsive while nearby pages are decoded in the background and reused from the
bounded display cache when available.
