# Research: Continuous Vertical Scroll

## Decision: Represent continuous reading as virtual page rectangles

**Rationale**: A virtual document made from page rectangles lets the reader compute total scroll height, visible intersections, and overdraw candidates without requiring every page texture to exist. The existing `continuous_canvas_height` helper already supports summed page heights and gaps, so the plan can extend that geometry with per-page offsets and viewport intersection helpers.

**Alternatives considered**:

- Render all pages into one large canvas: rejected because it would require preparing and retaining too many pages for large comics.
- Keep discrete page rendering and fake scroll events as page turns: rejected because it would not satisfy continuous uninterrupted reading.

## Decision: Keep page measurements separate from texture cache entries

**Rationale**: Continuous layout needs stable page heights even when texture resources are evicted from the bounded display cache. Storing lightweight page measurements independently allows layout stability without keeping every page resident in RAM/VRAM.

**Alternatives considered**:

- Derive all page sizes from cached textures: rejected because evicting a texture would make the layout forget its size and jump.
- Parse all archive page dimensions up front: rejected because it risks large archive I/O and parsing work before the reader can interact.

## Decision: Use placeholders based on first known page ratio

**Rationale**: The first successfully measured page gives a comic-local aspect ratio that is usually a better placeholder than a fixed global size. A stable portrait fallback covers the case where no page has been measured yet.

**Alternatives considered**:

- Use zero-height placeholders: rejected because the document would collapse and scrolling would be unstable.
- Use a fixed pixel height for all unknown pages: rejected because it ignores viewport width and creates poor estimates after resize.

## Decision: Prepare only viewport-intersecting pages plus one-page overdraw

**Rationale**: The spec requires visible pages plus one page above and below. This keeps scrolling responsive while giving adjacent content time to prepare before it enters view. It also keeps active work bounded for long archives.

**Alternatives considered**:

- Reuse the paged prefetch window `[n+1, n+2, n-1]` directly: rejected because continuous scroll visibility can include multiple pages and does not map cleanly to one active page.
- Prefetch all pages below the viewport: rejected because it violates bounded memory/work constraints.

## Decision: Disable side-by-side spread composition in continuous vertical mode

**Rationale**: The spec requires continuous vertical flow and states that two-page spread must not conflict with it. Treating spread as a paged-layout feature keeps behavior predictable and avoids mixing horizontal composition into a vertical document.

**Alternatives considered**:

- Pair portrait pages inside the continuous column: rejected because it changes vertical page count and complicates scroll anchoring.
- Let spread and continuous toggles both affect layout: rejected because it creates ambiguous UI and geometry.

## Decision: Preserve scroll location by anchoring to nearest visible page

**Rationale**: When real dimensions replace placeholders, total document height changes. Anchoring to the nearest visible page and its intra-page offset keeps the reader near the same content after recalculation.

**Alternatives considered**:

- Preserve absolute scroll offset only: rejected because earlier pages changing height would move the reader to different content.
- Always jump to current page top after recalculation: rejected as disruptive during progressive loading.

## Decision: Keep all archive and decode work off the render loop

**Rationale**: The constitution prohibits reader I/O and decoding on the egui thread. Continuous scroll may request many pages over time, so requests must use the existing background worker path and reconciliation model.

**Alternatives considered**:

- Synchronously load a newly visible page when drawing it: rejected because scroll input would hitch.
- Spawn ad hoc threads per page: rejected because the existing worker pool already provides bounded concurrency and cancellation.
