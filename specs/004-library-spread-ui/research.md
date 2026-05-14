# Research: Library Spread UI

## Decision: Treat spread mode as a pure display decision

**Rationale**: The rule is deterministic from current-page dimensions and next
page availability: landscape pages render alone; portrait or square pages pair
with the next page when possible. Keeping this as pure logic makes odd/even
page positions, last-page behavior, and missing-next-page recovery easy to test.

**Alternatives considered**: Storing precomputed spread groupings was rejected
for this feature because pairing rules are simple and page dimensions may only
be known once page resources are inspected.

## Decision: Store spread state in ViewerState, not LibraryService

**Rationale**: Spread mode is a reading interaction preference for the active
session. LibraryService should provide comic/page facts and persistence, while
ViewerState should decide how prepared page resources are composed on screen.

**Alternatives considered**: Persisting spread mode per comic was deferred; the
spec only requires a toggle while reading and does not define preference scope.

## Decision: Request next-page resources through the existing async pipeline

**Rationale**: Rendering a portrait spread needs the current page and next page,
but decoding and resizing still must happen outside the egui context. Reusing
the existing request/result pattern preserves UI responsiveness and cache
limits.

**Alternatives considered**: Synchronously reading or decoding the next page
during spread rendering was rejected as a direct constitution violation.

## Decision: Use filesystem watching as an event trigger, then reconcile by scanning

**Rationale**: Filesystem events can arrive in bursts, omit intermediate states,
or fire while a copy is still in progress. Treating watch events as a prompt to
rescan affected paths after a short settling window gives the library a stable
final view and avoids duplicate records.

**Alternatives considered**: Trusting individual create/remove events as the
source of truth was rejected because rename/copy/delete sequences are platform
dependent and can produce stale database rows.

## Decision: Keep SQLite writes behind LibraryService

**Rationale**: The constitution assigns metadata, folder tracking, progress,
and watcher-driven updates to LibraryService. Scanner and watcher workers
should send discovered changes to service-owned update methods rather than
writing storage directly from the UI or viewer.

**Alternatives considered**: Letting the app shell mutate storage directly was
rejected because it couples navigation state to persistence and makes tests
cross too many boundaries.

## Decision: Cache cover thumbnails on disk with source invalidation

**Rationale**: Cover extraction and resizing are expensive at startup. A local
thumbnail cache keyed by source archive identity and modification data avoids
re-extracting unchanged covers while still allowing refresh when archives
change.

**Alternatives considered**: Keeping thumbnails only in memory was rejected
because it does not satisfy restart behavior and scales poorly for large
libraries. Embedding thumbnail blobs in SQLite was deferred to keep the database
focused on metadata and avoid large-row churn.

## Decision: Build the library grid from lightweight view models

**Rationale**: The egui grid should only render already-known comic metadata
and prepared thumbnail handles. Missing thumbnails can show loading/error
states while worker results arrive. This keeps scrolling cheap and predictable.

**Alternatives considered**: Generating thumbnails while laying out grid tiles
was rejected because library scrolling must remain responsive.

## Decision: Introduce an explicit top-level AppState

**Rationale**: The application now has two first-class modes: browsing the
library and reading a selected comic. An enum makes transitions testable and
keeps update routing explicit.

**Alternatives considered**: Inferring mode from nullable selected comic fields
was rejected because it obscures transitions and tends to leak reader state into
library UI code.
