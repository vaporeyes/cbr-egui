# Quickstart: Library Metadata Surfacing

## Automated Validation

1. Run focused tests:

   ```sh
   cargo test --test library_storage --test library_scanner --test app_routing --test metadata_parser
   ```

2. Run the broader safety check:

   ```sh
   cargo clippy --all-targets -- -D warnings
   ```

## Manual Validation

1. Launch the app.
2. Scan a library containing comics with `ComicInfo.xml` metadata for series, number, writer, or penciller.
3. Confirm thumbnail cards and list rows show concise subtitles where metadata exists.
4. Select a series filter and confirm only matching comics remain visible.
5. Select a folder filter and confirm comics without series metadata remain discoverable.
6. Switch between thumbnail and list views while a filter is active and confirm the filter remains applied.
7. Clear the filter and confirm all loaded comics return.
8. Open a visible comic from a filtered view and confirm the reader opens normally.
9. Confirm unavailable comics remain marked unavailable and cannot be opened.
