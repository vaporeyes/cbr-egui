# Contract: Library Metadata UI

## Metadata Subtitle Contract

For every visible library item:
- If metadata includes issue number and writer, show a subtitle in the form `Issue #<number> - <writer>`.
- If metadata includes issue number but no writer, show `Issue #<number>`.
- If metadata includes writer but no issue number, show `<writer>`.
- If writer is unavailable and penciller is available, use the penciller as the creator field.
- If no useful metadata exists, show no subtitle line.
- Never show empty separators, raw `None`/`null` values, or placeholder text.

## Group Filter Contract

The library view exposes:
- A way to show all items.
- A way to select a series group when at least one series group exists.
- A way to select a folder group when at least one folder group exists.
- An item count for each group.

When a filter is active:
- Thumbnail and list views receive only matching items.
- Switching between thumbnail and list views keeps the selected filter.
- Clearing the filter restores all loaded library items.
- If the selected group disappears after a scan refresh, the active filter clears.

## Existing Behavior Contract

Filtering must not change:
- How a comic opens from the library.
- How unavailable comics are displayed or blocked from opening.
- How thumbnails are requested, loaded, or cached.
- How scan status and scan errors are shown.
