# Contract: Reader Prefetch Runtime

This contract describes how the active reader session, background decode workers, and texture cache interact. It is an internal application contract, not a network API.

## Dispatch Contract

**Trigger**: Active page changes or the active page becomes ready.

**Inputs**:

- Active comic path.
- Active page index.
- Total page count.
- Cached page indices.
- Queued page indices.
- In-flight page indices.

**Behavior**:

1. Cancel stale in-flight requests that are no longer useful for the active page.
2. Build nearby candidates in this order: next page, following page, previous page.
3. Exclude out-of-range, cached, queued, and in-flight pages.
4. Read candidate page bytes through the archive VFS.
5. Submit bounded background decode requests marked as prefetch work.
6. Store each request identity and cancellation handle in the in-flight set.

**Postconditions**:

- No candidate page is submitted more than once.
- Active in-flight pages are bounded by the nearby-page window.
- A queue-full condition leaves the page eligible for a later dispatch pass.

## Reconciliation Contract

**Trigger**: Each app update tick while a reader session is active.

**Inputs**:

- Background decode results.
- Current in-flight request map.
- Current page texture cache.
- UI context for texture upload.

**Behavior**:

1. Poll all available worker results without blocking.
2. Ignore results with unknown, stale, or cancelled request identities.
3. Convert successful image results into display textures on the UI thread.
4. Insert display textures into the bounded page cache.
5. Record or ignore recoverable failures without interrupting the visible page.
6. Remove reconciled pages from in-flight tracking.

**Postconditions**:

- Successful prepared pages are available for cache hits during navigation.
- Failed or cancelled background work cannot overwrite newer cached content.
- Cache size never exceeds configured capacity.

## Navigation Cache Contract

**Trigger**: User requests a page turn or direct page jump.

**Inputs**:

- Requested page index.
- Page texture cache.
- Active reader session.

**Behavior**:

1. Check the display cache before performing direct page loading.
2. If the requested page exists in cache, render it immediately and refresh cache recency.
3. If the requested page is not cached, load it through the direct visible-page path.
4. After navigation completes, dispatch prefetch for the new active page.

**Postconditions**:

- Cached page turns avoid repeated heavy preparation.
- Direct visible page load remains recoverable if a page is corrupt or unsupported.

## Cancellation Contract

**Trigger**: Active page changes, reader session closes, or active comic changes.

**Behavior**:

1. Determine which in-flight requests are no longer candidates for the new active page.
2. Invoke their cancellation handles.
3. Remove cancelled requests from queued/in-flight tracking.
4. Ignore any late results from cancelled requests.

**Postconditions**:

- Stale work does not delay useful nearby-page preparation.
- Session teardown leaves no live request handles owned by the reader session.
