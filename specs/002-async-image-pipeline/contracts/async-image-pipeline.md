# Contracts: Asynchronous Image Pipeline

## Decode Pipeline Contract

The decode pipeline accepts raw page bytes from VFS-backed page loading and
returns display-ready image results. It must not read archive files directly.

```rust
pub struct DecodeRequest {
    pub request_id: DecodeRequestId,
    pub page_index: usize,
    pub bytes: Vec<u8>,
    pub purpose: DecodePurpose,
    pub target_size: Option<[u32; 2]>,
}

pub enum DecodePurpose {
    Direct,
    Prefetch,
}

pub struct DecodeResult {
    pub request_id: DecodeRequestId,
    pub page_index: usize,
    pub outcome: Result<egui::ColorImage, DecodeError>,
}

pub fn decode_page(request: DecodeRequest) -> DecodeResult;
```

**Required behavior**

- Use raw bytes supplied by callers, normally read through `ArchiveReader`.
- Decode and optional high-quality downsampling happen outside the egui render
  loop.
- Successful results contain `ColorImage`.
- Corrupt, unsupported, empty, or oversized image failures return `DecodeError`
  rather than panicking.

## Worker Pool Contract

```rust
pub struct WorkerPool { /* worker-owned internals */ }

impl WorkerPool {
    pub fn start(worker_count: usize, queue_bound: usize) -> Result<Self, WorkerError>;
    pub fn submit(&self, request: DecodeRequest) -> Result<(), WorkerError>;
    pub fn try_recv(&self) -> Option<DecodeResult>;
    pub fn shutdown(self) -> Result<(), WorkerError>;
}
```

**Required behavior**

- Starts at least one dedicated decode worker thread.
- Uses bounded channels for request submission and result delivery.
- Allows the main thread to poll results without blocking.
- Preserves request identity in every result.
- Reports worker shutdown/submission failures as recoverable errors.

## Prefetch Scheduler Contract

```rust
pub struct PrefetchState {
    pub page_count: usize,
    pub cached: std::collections::HashSet<usize>,
    pub queued: std::collections::HashSet<usize>,
    pub in_flight: std::collections::HashSet<usize>,
}

pub fn prefetch_candidates(current_page: usize, state: &PrefetchState) -> Vec<usize>;
```

**Required behavior**

- Candidate order is `current_page + 1`, `current_page + 2`,
  `current_page - 1`.
- Out-of-range indices are skipped.
- Cached, queued, and in-flight page indices are skipped.
- The function is deterministic and independently testable.

## Display Cache Contract

```rust
pub struct PageTextureCache<T> { /* LRU internals */ }

impl<T> PageTextureCache<T> {
    pub fn new(capacity: usize) -> Result<Self, CacheError>;
    pub fn get(&mut self, page_index: usize) -> Option<&T>;
    pub fn insert(&mut self, page_index: usize, texture: T) -> Option<T>;
    pub fn contains(&self, page_index: usize) -> bool;
    pub fn len(&self) -> usize;
    pub fn capacity(&self) -> usize;
}
```

**Required behavior**

- Capacity must be at least 1 and at most 10.
- Default capacity is 5.
- Insertion beyond capacity evicts the least-recently-used entry.
- Access refreshes recency.
- Texture handles are inserted only after main-thread texture creation.

## Viewer Integration Contract

ViewerState is responsible for:

- Tracking current page generation/request identity.
- Submitting direct and prefetch decode requests.
- Polling worker results without blocking the render loop.
- Ignoring stale results that no longer match the active comic/page generation.
- Creating `egui::TextureHandle` from returned `ColorImage` on the main thread.
- Inserting texture handles into `PageTextureCache`.

LibraryService and VFS remain responsible for library metadata and archive page
bytes; decode and viewer modules must not bypass those boundaries.
