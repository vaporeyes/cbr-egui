# Contracts: egui Viewer Implementation

## Layout Contract

```rust
pub enum ViewMode {
    Fit,
    Fill,
}

pub struct Size2 {
    pub width: f32,
    pub height: f32,
}

pub fn page_display_size(page: Size2, viewport: Size2, mode: ViewMode) -> Size2;
```

**Required behavior**

- Fit mode preserves aspect ratio and keeps the full page visible.
- Fill mode preserves aspect ratio and may exceed one viewport axis.
- Zero or invalid dimensions return a safe zero/empty size rather than
  panicking.

## Zoom and Pan Contract

```rust
pub struct ZoomPanState {
    pub zoom: f32,
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub reset_generation: u64,
}

impl ZoomPanState {
    pub fn apply_scroll_zoom(&mut self, scroll_delta: f32);
    pub fn apply_drag_pan(&mut self, drag_delta: [f32; 2], page_size: Size2, viewport: Size2);
    pub fn reset_for_page(&mut self, page_id: PageId);
}
```

**Required behavior**

- Zoom is clamped between configured min and max.
- Pan is clamped to page bounds after zoom or drag.
- Pan remains zero when the page does not exceed viewport dimensions.
- Page identity change resets zoom and pan.

## ViewerState Contract

```rust
pub enum PageStatus<T> {
    Empty,
    Loading,
    Ready { page_id: PageId, texture: T, pixel_size: Size2 },
    Failed { page_id: PageId, message: String },
}

pub struct ViewerState<T> {
    pub current_page_id: Option<PageId>,
    pub view_mode: ViewMode,
    pub zoom_pan: ZoomPanState,
    pub page_status: PageStatus<T>,
}
```

**Required behavior**

- Current page updates reset zoom and pan when the page id changes.
- Ready pages are rendered from prepared texture handles supplied by the async
  image pipeline/cache.
- Loading and failed states are recoverable and non-panicking.
- ViewerState does not call LibraryService or archive readers directly.

## egui Rendering Contract

The UI integration must:

- Render inside the central reading area.
- Display the current texture with aspect-ratio-preserving fit/fill sizing.
- Use scroll-area behavior for zoomed content and pan interaction.
- Update only viewer state during pointer/scroll handling.
- Avoid decode, archive read, resize, and texture upload work inside
  scroll/drag handlers.
- Keep chrome minimal and non-obstructive during normal reading.
