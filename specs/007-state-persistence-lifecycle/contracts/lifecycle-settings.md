# Contract: Lifecycle Persistence and Settings UI

## Startup Contract

1. Load app configuration from the resolved config path.
2. Apply visual appearance before first visible frame.
3. Construct LibraryService and load library items from persisted storage when available.
4. Attempt to resume the last valid session.
5. If resume succeeds, set app state to reader for the saved comic and clamp the target page.
6. If resume fails or no session exists, show library view.

**Fallbacks**:

- Missing or invalid config uses defaults.
- Missing or unavailable last comic routes to library.
- Startup errors are represented as non-blocking status text where possible.

## Save Lifecycle Contract

Triggered by the desktop app lifecycle save hook.

1. Serialize current AppConfig to the config path.
2. If a reading session is active, write progress through LibraryService.
3. Persist `current_page_index`, not only the last decoded/ready page.
4. Mark read status when the current page is the final page.
5. Record non-fatal save errors for later display; do not panic.

**Must Not Do**:

- No archive reads.
- No image decoding or resizing.
- No thumbnail scans.
- No direct ViewerState-to-database writes.

## Settings Window Contract

Entry point: toolbar settings control in library and reader contexts.

Controls:

- Appearance: dark/light toggle, applied immediately.
- Zoom sensitivity: bounded numeric/slider control, affects future zoom gestures.
- Reading direction: LTR/RTL selector, applied to new sessions and current session where applicable.

Behavior:

- Window is non-destructive and can be opened/closed repeatedly.
- Current reading page and cache state remain intact.
- Config changes persist during lifecycle save and may be saved immediately if error handling remains non-blocking.

## Error Contract

- Config save failure: keep app usable and show a non-blocking status/warning.
- Progress save failure: keep app usable and show a non-blocking status/warning on next update.
- Config load failure: use defaults.
- Resume failure: route to library.
