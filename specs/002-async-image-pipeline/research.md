# Research: Asynchronous Image Pipeline

## Decision: Use `std::thread` workers with `crossbeam_channel`

**Rationale**: The constitution requires reader CPU/I/O work to default to
`std::thread` and `crossbeam_channel`. This feature has no network or
server-mode need for tokio, and bounded message channels make backpressure and
worker shutdown explicit.

**Alternatives considered**: Tokio was rejected because no async runtime owner
is needed. Running decode work on egui update was rejected because it violates
UI responsiveness requirements.

## Decision: Decode raw page bytes with `image` into `egui::ColorImage`

**Rationale**: The user request explicitly needs raw bytes decoded through the
image pipeline and returned as a display-ready image. The `image` crate handles
common comic page formats and can resize before conversion to `ColorImage`,
keeping expensive work off the main thread.

**Alternatives considered**: Passing decoded `DynamicImage` to the main thread
was rejected because it postpones conversion/resizing work into UI code. Manual
format decoders were rejected because maintained crate support is sufficient.

## Decision: Create `TextureHandle` only on the main thread

**Rationale**: Texture upload depends on the egui context and must remain owned
by the main UI thread. Workers return `ColorImage` results only; ViewerState
polls results, uploads textures, and inserts handles into its cache.

**Alternatives considered**: Giving workers access to the egui context was
rejected because it mixes UI state with background work and risks thread-safety
violations.

## Decision: Prefetch fixed nearby window in order `n+1`, `n+2`, `n-1`

**Rationale**: The requested order prioritizes normal forward reading, then a
short backward correction. A small fixed window satisfies the constitution's
bounded memory guidance and is easy to test independently.

**Alternatives considered**: Larger adaptive windows were rejected for this
feature because they increase memory pressure and require usage analytics that
are out of scope.

## Decision: Track queued and in-flight page indices to suppress duplicates

**Rationale**: Rapid page changes can otherwise enqueue the same nearby page
many times. Tracking cached, queued, and in-flight indices lets the scheduler
skip redundant work while still accepting direct page requests.

**Alternatives considered**: Allowing duplicate work was rejected because it
wastes CPU and can delay current-page requests behind stale prefetches.

## Decision: Use a strict LRU cache with default capacity 5 and max 10

**Rationale**: The constitution caps full-page cache growth unless explicitly
justified. Five pages covers current, previous, and the small prefetch window
with room for one recent page; max 10 leaves controlled flexibility.

**Alternatives considered**: Unbounded caches were rejected. A simple FIFO cache
was rejected because frequently revisited pages should remain hot.

## Decision: Treat decode failures and stale results as recoverable outcomes

**Rationale**: Real archives contain corrupt or unsupported images, and rapid
navigation can make older results irrelevant. Typed result states let the viewer
show or log recoverable errors and ignore stale images without panics.

**Alternatives considered**: Panicking on decode failure or blindly inserting
all returned images were rejected because they break navigation continuity and
can show the wrong page.
