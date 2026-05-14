# Quickstart: Library Spread UI

## Prerequisites

- Rust 2024 toolchain available through Cargo.
- A local folder containing at least one supported comic archive.
- At least one archive with standard portrait pages.
- Optional: one archive or page fixture containing a landscape pre-stitched
  spread and one corrupt or unsupported archive for failure validation.

## Validate the Plan

1. Check the crate builds:

   ```bash
   cargo check
   ```

2. Run the full test suite:

   ```bash
   cargo test
   ```

3. Run focused tests expected from this feature once tasks are implemented:

   ```bash
   cargo test --test spread_viewer
   cargo test --test library_scanner
   cargo test --test library_watcher
   cargo test --test thumbnail_cache
   cargo test --test app_routing
   ```

## Manual Smoke Check

1. Configure a library root containing several supported archives.
2. Confirm the library grid displays cover tiles and remains responsive while
   covers are generated.
3. Restart the application and confirm unchanged covers appear from cache
   without repeated extraction.
4. Add a new archive to the root and confirm it appears after the file is
   stable.
5. Remove an archive and confirm the library no longer offers it as a normal
   readable item.
6. Open a comic from the grid and return to the library.
7. In the reader, toggle spread mode:
   - A landscape pre-stitched page renders alone.
   - A portrait page renders with the next page when available.
   - The last page renders alone without error.
8. Confirm zoom and pan reset when changing pages or spread composition.
9. Confirm corrupt covers/pages show recoverable states and do not prevent
   browsing or navigation.

## Boundary Checks

Run these inspections after implementation:

```bash
rg -n "rusqlite|notify|scanner|watcher|LibraryService" src/viewer
rg -n "load_from_memory|resize\\(|\\bread_page\\b|\\bread_entry\\b|SQLite|rusqlite|write_thumbnail" src/app src/viewer
rg -n "LibraryService" src/viewer
```

Expected result: viewer and app rendering paths should not perform storage,
watching, archive reads, image decoding, or resizing directly. They should
consume prepared state and route commands to library/decode services.
