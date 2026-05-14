# Data Model: egui Viewer Implementation

## ViewerState

Represents current reading UI state.

**Fields**

- `current_page_id`: Identity of the displayed page, if any.
- `view_mode`: Fit or fill behavior for the page.
- `zoom_pan`: Current ZoomPanState.
- `page_status`: Whether a page is loading, available, or failed.
- `viewport_size`: Last known reading area size.

**Relationships**

- Consumes prepared page display resources from the async image pipeline/cache.
- Owns reading interaction state.
- Does not own library persistence or archive byte access.

**Validation**

- Current page changes reset zoom and pan state.
- ViewerState updates must remain cheap enough to inspect every frame.

**State Transitions**

- No page -> loading when a page is requested.
- Loading -> available when a texture is ready.
- Loading -> failed when page preparation fails.
- Available -> loading or available with reset zoom/pan when page changes.

## PageViewport

Represents the reading area available for page display.

**Fields**

- `width`
- `height`

**Relationships**

- Used with page dimensions to compute fit and fill sizes.
- Drives pan bounds when zoomed page dimensions exceed visible dimensions.

**Validation**

- Width and height must be positive before layout calculations.
- Very small viewports produce a minimum safe display size rather than invalid
  geometry.

## PageDisplayResource

Represents a prepared page image resource ready for display.

**Fields**

- `page_id`
- `pixel_width`
- `pixel_height`
- `texture_handle`

**Relationships**

- Produced by async image pipeline/cache.
- Consumed by viewer rendering integration.

**Validation**

- Pixel dimensions must be positive.
- Texture handle creation remains outside background workers.

## ZoomPanState

Represents per-page scale and visible-region state.

**Fields**

- `zoom`: Multiplier over default fitted page size.
- `min_zoom`
- `max_zoom`
- `pan_offset`
- `reset_generation`

**Relationships**

- Owned by ViewerState.
- Updated by scroll and drag input.
- Reset when current page identity changes.

**Validation**

- Zoom is clamped between min and max.
- Pan offset is clamped to useful page bounds.
- Pan returns to zero when page fits within the viewport.

## ViewerChrome

Represents minimal reading affordances around the page.

**Fields**

- `visible`
- `status_text`
- `control_region`

**Relationships**

- Owned by viewer rendering integration.
- Reflects page status and mode without obscuring content.

**Validation**

- Chrome must not cover page content in normal reading mode.
- Missing/error states may use central recoverable messaging.
