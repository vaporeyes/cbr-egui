# Contract: Library Shell

## Purpose

Define the collection browsing, filesystem synchronization, thumbnail cache, and
application routing behavior for the navigational shell.

## Inputs

- Configured library root path.
- Supported archive files under the root.
- Filesystem change notifications.
- Existing library records.
- Existing thumbnail cache files and source fingerprints.
- User actions: select comic, return to library, toggle spread mode.

## Outputs

- Updated comic records with availability and page counts.
- Coalesced scan status for the library root.
- Cover thumbnail statuses: missing, loading, ready, failed, stale.
- Library grid view model.
- Application state transitions between library browsing and reading.

## Required Behavior

1. A configured root scan discovers supported archives recursively and updates
   collection records by unique path.
2. File additions, removals, and changes trigger reconciliation after events
   settle.
3. Reconciliation updates final records from stable filesystem state and avoids
   duplicates.
4. Cover thumbnail generation uses the first usable page when no valid cached
   thumbnail exists.
5. Thumbnail display height is capped at 300px.
6. Thumbnail cache entries are reused across restarts when source fingerprints
   still match.
7. Stale thumbnail cache entries are regenerated.
8. The library grid wraps tiles responsively as available width changes.
9. Selecting an available comic routes to reading state for that comic.
10. Returning from the reader routes back to the library state with collection
    data intact.

## Failure Behavior

- Inaccessible root: report recoverable library status and keep app usable.
- Corrupt archive: keep or mark comic with recoverable error; do not block other
  comics.
- Missing cover page: show placeholder/error tile and allow other comics to
  load.
- Watch event overflow or burst: schedule full root reconciliation.
- Thumbnail write failure: show a placeholder and allow retry.

## Test Expectations

- Added archives appear after scan/reconciliation.
- Removed archives disappear or become unavailable according to storage policy.
- Repeated watch events for the same file do not create duplicate records.
- Unchanged cached thumbnails are reused after restart.
- Changed source archives invalidate cached thumbnails.
- Library-to-reader and reader-to-library transitions are explicit and
  reversible.
