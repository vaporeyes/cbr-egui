# Data Model: State Persistence & Lifecycle Management

## AppConfig

**Purpose**: Persist user preferences that affect the app shell and reader behavior.

**Fields**:

- `dark_mode: bool`: true for dark appearance, false for light appearance.
- `zoom_sensitivity: f32`: multiplier used for future zoom gestures.
- `reading_direction: ReadingDirection`: default direction for reading sessions.

**Validation Rules**:

- Missing config file uses defaults.
- Invalid config content uses defaults.
- `zoom_sensitivity` must be finite and positive; implementation should clamp to a small safe range.
- Reading direction must deserialize to a supported enum value.

**State Transitions**:

- Loaded on startup before first render.
- Mutated by settings window.
- Serialized during lifecycle save.
- May be saved opportunistically on settings changes if implementation keeps that path non-blocking.

## ReadingProgress

**Purpose**: Persist the user's current position for a comic.

**Fields**:

- `comic_id: i64`: identifies the comic.
- `current_page: u32`: last intended page index.
- `is_read: bool`: whether the comic is complete/read.

**Validation Rules**:

- Page index is clamped to the available page range when resuming.
- Repeated saves for the same comic update the existing row instead of creating duplicates.
- Progress writes are skipped when no reading session is active.

**State Transitions**:

- Created or updated when lifecycle save runs with an active session.
- Read during startup resume selection.
- Updated during future navigation checkpoints if tasks choose to add opportunistic saves.

## LastSession

**Purpose**: Represent the most recent valid session eligible for automatic resume.

**Fields**:

- `comic`: available comic metadata.
- `progress`: reading progress for that comic.

**Validation Rules**:

- Comic must be available.
- Comic must have at least one page.
- Progress page must be clamped if it exceeds current page count.

**State Transitions**:

- Derived from LibraryService during startup.
- Opens app state to `Reading(comic_id)` when valid.
- Falls back to Library when absent or invalid.

## SettingsWindowState

**Purpose**: Track transient UI state for the preferences modal.

**Fields**:

- `open: bool`: whether the window is visible.
- `last_error: Option<String>`: non-blocking persistence/config warning, if any.

**Validation Rules**:

- Opening or closing settings must not reset reading position.
- Preference changes must update the live app config immediately.

**State Transitions**:

- Opens from toolbar settings control.
- Mutates AppConfig as controls change.
- Closes without discarding already-applied changes.
