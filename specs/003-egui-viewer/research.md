# Research: egui Viewer Implementation

## Decision: Default to fit-to-page, with explicit fill mode

**Rationale**: Fit-to-page keeps the entire comic page visible and satisfies the
primary reading scenario without surprise cropping. Fill mode can make better
use of the window for immersive reading, but only when explicitly selected.

**Alternatives considered**: Fill-by-default was rejected because it can hide
panels or text on pages whose aspect ratio does not match the viewport.

## Decision: Keep layout math pure and separate from egui rendering

**Rationale**: Aspect-ratio sizing, zoom clamping, pan bounds, and reset
behavior can be tested without a GUI context. This keeps the render integration
small and reduces the chance of blocking or state-heavy code inside the update
loop.

**Alternatives considered**: Embedding all math directly in the UI function was
rejected because it would make core behavior hard to test and easy to regress.

## Decision: Represent zoom as a multiplier over fitted page size

**Rationale**: A multiplier keeps resize behavior intuitive: the base fit size
updates with the viewport, and the user's zoom is applied consistently above
that baseline. It also makes page-turn reset straightforward.

**Alternatives considered**: Absolute pixel zoom was rejected because it behaves
poorly when the window is resized and makes cross-page reset less predictable.

## Decision: Clamp zoom and pan to bounded values

**Rationale**: Zoom limits prevent unusable tiny or excessively large pages.
Pan bounds prevent the viewport from drifting away from useful content, which
keeps repeated drag interactions stable.

**Alternatives considered**: Unbounded pan/zoom was rejected because it creates
lost-content states and violates predictable reading interaction.

## Decision: Reset zoom and pan on page identity change

**Rationale**: Carrying zoom/pan state across pages is disorienting, especially
when page aspect ratios differ. Resetting gives each page a readable starting
point and matches the requested edge case.

**Alternatives considered**: Preserving per-page zoom history was deferred; it
requires a policy for memory and cross-page expectations that is outside this
single-page viewer feature.

## Decision: Use minimal non-obstructive chrome

**Rationale**: The user asked for a sleek modern reading interface. A
content-first view with restrained status affordances avoids obscuring art or
text and keeps the feature focused on reading rather than full app navigation.

**Alternatives considered**: Persistent heavy toolbars were rejected because
they reduce reading space and make the first viewer feel more like a utility
panel than a reader.

## Decision: Show recoverable loading/error states for missing page resources

**Rationale**: The async image pipeline may not have produced a texture yet, or
the page may fail. A recoverable presentation keeps the viewer alive and lets
navigation continue.

**Alternatives considered**: Panicking or showing a blank central panel was
rejected because both obscure actionable state and break reader confidence.
