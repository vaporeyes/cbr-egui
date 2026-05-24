# Research: State Persistence & Lifecycle Management

## Decision: `EguiComicReaderApp` owns lifecycle persistence inputs

**Rationale**: The eframe lifecycle hooks are implemented on the app wrapper, so it is the narrowest place that can coordinate config saving, progress flushing, settings UI state, and active reading state without leaking persistence into ViewerState.

**Alternatives considered**:

- Store persistence logic in ViewerState: rejected because it violates the LibraryService/ViewerState boundary.
- Store persistence logic only in `main.rs`: rejected because save hooks execute on the app instance after construction.

## Decision: Keep progress writes behind LibraryService

**Rationale**: The constitution assigns progress and SQLite ownership to LibraryService/storage. Existing `save_progress` and `last_read_comic` APIs already provide the needed contract.

**Alternatives considered**:

- Write directly to storage from UI code: rejected because it bypasses the service boundary.
- Cache progress only in memory and rely on explicit navigation saves: rejected because forced exits can lose the active page.

## Decision: Save the intended current page from ReadingSession

**Rationale**: `ReadingSession::current_page_index` reflects the user's navigation target even if page texture loading is still pending. This satisfies the edge case where the app exits before a page fully loads.

**Alternatives considered**:

- Save only when `PageStatus::Ready`: rejected because it can regress to an older page on slow or failed loads.
- Save only when returning to library: rejected because direct app exits would lose progress.

## Decision: Invalid config falls back to defaults

**Rationale**: `AppConfig::load` already returns defaults on missing or invalid content, which matches the spec's safe-startup requirement.

**Alternatives considered**:

- Blocking startup with an error dialog: rejected because corrupt settings should not prevent reading.
- Partially preserving invalid values: rejected because zoom sensitivity and direction need known-safe ranges.

## Decision: Settings changes apply immediately and persist on save

**Rationale**: Theme changes can be applied through egui visuals, reading direction can be applied to current/new viewer state, and zoom sensitivity should affect future zoom gestures without resetting the current zoom level.

**Alternatives considered**:

- Require restart for all settings: rejected because the spec requires immediate appearance updates.
- Auto-save config on every widget change only: acceptable as a future enhancement, but lifecycle save is the required durability hook.

## Decision: Startup resume is best-effort

**Rationale**: Missing libraries, unavailable comics, and invalid progress must not block launch. If a session cannot be resumed safely, the app opens to Library.

**Alternatives considered**:

- Prompt during startup to locate missing content: rejected for this feature because the spec assumes a safe library fallback.
- Open a reader error page for unavailable comics: rejected because the reader should not show a broken session.
