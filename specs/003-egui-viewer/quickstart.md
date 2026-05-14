# Quickstart: egui Viewer Implementation

## Prerequisites

- Rust 2024 toolchain available through Cargo.
- Async image pipeline feature available to provide prepared page textures.

## Validate the Feature

1. Check the crate builds:

   ```bash
   cargo check
   ```

2. Run the focused test suite:

   ```bash
   cargo test
   ```

3. Layout validation expected from tests:

   - Fit mode keeps portrait, landscape, square, tall, and spread pages fully
     visible.
   - Fill mode preserves aspect ratio while using more of the viewport.
   - Resize-driven calculations never stretch the page.

4. Interaction validation expected from tests:

   - Scroll input clamps zoom between configured min and max.
   - Drag input pans only when the zoomed page exceeds the viewport.
   - Pan offset is clamped to useful page bounds.
   - Page identity changes reset zoom and pan state.

5. Boundary validation expected from tests or code inspection:

   - Viewer UI uses prepared textures from the async image pipeline/cache.
   - Viewer modules do not perform archive reads, image decoding, image
     resizing, SQLite scans, or texture upload during scroll/drag handling.
   - Missing/loading/error page states are recoverable.

## Manual Smoke Check

After the viewer is wired to sample prepared textures, open a portrait page,
resize the window, zoom in with the scroll wheel, drag to pan, then change pages.
The page should remain aspect-correct during resize, zoom/pan should feel stable,
and the next page should return to the default fitted view.
