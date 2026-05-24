# Research: Library Metadata Surfacing

## Decision: Build Grid Rows From Metadata-Aware Library Service Results

**Rationale**: The library service already owns SQLite access and returns `LibraryGridItem` values consumed by app state. Extending that service output keeps metadata formatting out of the viewer and avoids direct storage calls from UI code.

**Alternatives considered**:
- Query metadata directly from egui UI: rejected because it violates the LibraryService boundary and risks render-loop SQLite work.
- Keep `LibraryGridItem` unchanged and perform lookups by comic ID later: rejected because it splits display data across multiple sources and complicates filtering.

## Decision: Use a LEFT JOIN for Metadata in Library Storage

**Rationale**: Comics may have no metadata, and those comics must stay visible. A left join preserves all comics while populating optional metadata fields when present.

**Alternatives considered**:
- Inner join: rejected because it would hide comics without metadata.
- Separate query per comic: rejected because it creates unnecessary database work and scales poorly for large libraries.

## Decision: Store Filter State in LibraryViewState

**Rationale**: The active filter is a session-scoped library browsing concern. Keeping it beside library items and view mode makes thumbnail/list rendering use the same filter without affecting persisted config or reader state.

**Alternatives considered**:
- Persist filter in app config: rejected for this phase because the specification scopes filters to the current session.
- Store filter in UI-only controls: rejected because filtering needs to be testable separately from egui rendering and stable across view mode changes.

## Decision: Filter In Memory Over Hydrated Items

**Rationale**: The specification asks for an in-memory filter, and the library already hydrates items for display. Filtering a vector of lightweight item records avoids per-frame database access and keeps list/grid behavior consistent.

**Alternatives considered**:
- Re-query SQLite on each filter change: rejected because it adds storage complexity and risks UI stalls.
- Maintain separate duplicated item lists for every group: rejected because it increases state synchronization work after scans.

## Decision: Normalize Group Keys, Preserve Display Labels

**Rationale**: Series names can vary by case or whitespace. Normalized keys allow stable grouping while display labels remain user-friendly.

**Alternatives considered**:
- Case-sensitive grouping: rejected because it creates duplicate groups for obvious metadata variants.
- Aggressive fuzzy matching: rejected because it may merge unrelated series and is outside this phase.

## Decision: Folder Group Uses Parent Directory Display Name

**Rationale**: Folder fallback must be available for comics without series metadata and is useful for collections organized by directory. The parent directory name is concise enough for a dropdown/sidebar while the full path can remain available internally as the group key.

**Alternatives considered**:
- Use full paths as visible labels: rejected because long paths are hard to scan.
- Use root-relative paths only: deferred because library root ownership is not yet modeled consistently in grid items.
