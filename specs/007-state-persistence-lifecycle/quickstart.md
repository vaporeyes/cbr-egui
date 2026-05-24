# Quickstart: State Persistence & Lifecycle Management

## Prerequisites

- A local comic library with at least one readable archive.
- Existing test archive fixtures are sufficient for automated tests.

## Automated Validation

Run:

```bash
cargo test --test app_routing --test app_config --test library_scanner
cargo clippy --all-targets -- -D warnings
```

Expected results:

- Active reading session progress is saved through the lifecycle path.
- Startup resume opens a valid saved comic at the saved page.
- Missing/invalid config falls back to defaults.
- Settings changes mutate app config and viewer defaults without resetting active reading position.

## Manual Validation

1. Start the app with an existing library.
2. Open a comic and navigate past the first page.
3. Close the app normally.
4. Reopen the app.
5. Confirm the app opens the same comic at the saved page.
6. Open settings from the toolbar.
7. Toggle dark/light appearance and confirm the app updates immediately.
8. Change zoom sensitivity and confirm future zoom gestures use the new speed.
9. Switch reading direction and confirm a newly opened reader uses the selected direction.
10. Corrupt or remove the config file and restart; confirm the app still launches.

## Regression Checks

- Returning to Library still cancels in-flight prefetch work.
- Continuous and paged reader modes still load pages through existing VFS/decode paths.
- Save lifecycle does not trigger archive extraction, image decoding, thumbnail generation, or scans.
