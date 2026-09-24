# Reader review remediation

Scope: address the code review findings without adding EPUB support or changing
unrelated work. Existing library rows, progress, and bookmarks must survive.

## 1. Lifecycle and durability

- [x] Flush application state through the actual shutdown callback.
- [x] Disconnect worker result queues during shutdown and bound their capacity.
- [x] Stage and hash imports before publishing; repair incomplete destinations.
- [x] Atomically replace configuration files.

Validation: shutdown callback, undrained queues, interrupted import recovery,
and repeated configuration replacement regression tests.

## 2. Resource boundaries and background work

- [x] Limit archive entries, metadata, document inputs, and raster dimensions.
- [x] Bound texture bytes, sidebar thumbnails, and queued decode results.
- [x] Keep queue saturation and initial document enumeration off the UI thread.
- [x] Persist import/rescan batches on a background database connection.
- [x] Stop automatic retries of terminal page failures.

Validation: oversized headers and entries, queue backpressure, cache eviction,
worker scheduling, and failed-page regression tests.

## 3. Reader and library correctness

- [x] Apply preferences whenever a reading session is created.
- [x] Move the continuous viewport for page and scroll navigation.
- [x] Keep page dimensions paired with textures in RTL spreads.
- [x] Preserve source folders separately from managed storage paths.
- [x] Deduplicate imports by content while preserving existing reading state.
- [x] Accept ZIP/RAR for explicit opens while keeping folder discovery selective.

Validation: promote the review probes into permanent regression tests; test
schema migration and imports with renamed identical content.

## 4. Backend data flow and verification

- [x] Pass rendered PDF/DjVu pixels directly to image processing.
- [x] Retain ZIP handles and propagate RAR enumeration errors.
- [x] Quote executable paths literally in the macOS launcher.
- [x] Run all targets, strict Clippy, formatting, and diff checks.

Validation uses the installed macOS 15.4 SDK when the system SDK is incompatible
with the local linker. Record any runtime-backend limitations explicitly.

## Validation results

- All 232 tests pass with `cargo test --all-targets`, including 27 added
  regression tests.
- `cargo clippy --all-targets -- -D warnings` passes.
- Changed Rust files pass rustfmt; `git diff --check` passes.
- Checks used `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk`.
  The default installed SDK is incompatible with the local linker.

The permanent tests are in `tests/review_regressions.rs` and
`tests/resource_regressions.rs`, with background import/rescan and launcher
tests next to their implementations. They cover all original reproduction
probes plus byte budgets, malformed archive headers, actual shutdown hooks,
queue backpressure, transaction rollback, and legacy schema migration.

## Behavior and limits

- Viewing uses background page enumeration, extraction, and decoding. Explicit
  page export still performs its requested read synchronously.
- Page textures have a 256 MiB cache budget; decoded output is capped at eight
  million pixels and 8192 pixels per side. Continuous prefetch uses a 1920-pixel
  bounding box so the sixteen-page working set fits the budget. These limits
  bound application buffers, not the total process or native-library memory.
- Archive entries are limited to 64 MiB, ComicInfo XML to 1 MiB, and PDF/DjVu
  inputs to 256 MiB. DjVu parser limits also bound decoded data and components.
  Larger files produce recoverable errors.
- File import stages and validates copied bytes before atomic publication. The
  database batch is transactional. File publication and database commit are
  separate operations; a rescan recovers a published copy after a crash between
  them. Directory entries are not explicitly fsynced, so atomic replacement is
  not a guarantee against every power-loss scenario.
- Existing comic IDs, progress, and bookmarks survive migration and reimport.
  Original folders that were never recorded cannot be reconstructed from
  legacy hash directories; those rows show no invented folder. Reimporting an
  original file records its source.
- ZIP/RAR are accepted for explicit opens and stored as CBZ/CBR so managed-store
  rescans retain them. Folder discovery keeps its narrower comic extensions.
- DjVu rendering is exercised using generated documents. PDF rendering changes
  compile, but a real PDFium rendering smoke test remains unverified on this
  machine; existing tests cover recoverable backend/file errors.
- EPUB is absent from this checkout. Adding its parsing, layout, and navigation
  is separate feature work, not part of these defect fixes.
