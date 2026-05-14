# Quickstart: Domain Models & Data Layer

## Prerequisites

- Rust 2024 toolchain available through Cargo.
- `7z` available on `PATH` for RAR/CBR listing and on-demand entry streaming.
  If it is missing, the RAR reader returns a recoverable backend-unavailable
  error.

## Validate the Feature

1. Check the crate builds:

   ```bash
   cargo check
   ```

2. Run the focused test suite:

   ```bash
   cargo test
   ```

3. Storage validation expected from tests:

   - Initialize a temporary SQLite database.
   - Create a root folder and nested folder.
   - Insert at least two comics.
   - Save progress twice for one comic.
   - Confirm only one progress row exists and the latest page/read state is
     returned.

4. Metadata validation expected from tests:

   - Parse a valid ComicInfo.xml containing `Title`, `Number`, `Writer`, and
     `Penciller`.
   - Parse missing optional fields as empty values.
   - Return a recoverable malformed metadata error for invalid XML.
   - Confirm page listing still works after metadata parse failure.

5. Archive VFS validation expected from tests:

   - Build a ZIP fixture containing `page_1.jpg`, `page_2.jpg`, and
     `page_10.jpg`.
   - Confirm listed pages are ordered `page_1`, `page_2`, `page_10`.
   - Confirm hidden metadata directories and non-image entries are absent.
   - Read one page by path and compare returned bytes with the fixture payload.
   - Run equivalent RAR coverage when `7z` is available; otherwise verify the
     backend-unavailable error is recoverable.

## Manual Smoke Check

Use a small CBZ/CBR fixture with ComicInfo.xml and three image pages. The data
layer should list pages, parse available metadata, and retrieve a single page
without extracting the full archive to a normal filesystem location.
