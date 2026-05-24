# Data Model: Library Metadata Surfacing

## LibraryGridItem

Represents a comic entry displayed in the library.

**Fields**:
- `comic_id`: stable comic identifier.
- `title`: primary display title.
- `subtitle`: optional metadata summary shown under the title.
- `path`: source path used for archive loading and folder grouping.
- `page_count`: number of readable pages.
- `thumbnail_status`: current cover thumbnail state.
- `availability`: available or unavailable.
- `series`: optional original series display value.
- `series_key`: optional normalized series grouping key.
- `folder_label`: parent folder display label.
- `folder_key`: normalized or full parent folder grouping key.

**Validation Rules**:
- `subtitle` must be absent when no useful metadata fields are available.
- `subtitle` must not contain duplicate separators or placeholder labels.
- `series_key` must be absent when series is blank after trimming.
- `folder_key` must be present when a parent folder can be derived from the source path.

## ComicMetadataDisplay

Display-ready metadata attached to a library item.

**Fields**:
- `title`: optional metadata title.
- `number`: optional issue number.
- `writer`: optional primary writer.
- `penciller`: optional primary penciller.

**Validation Rules**:
- Blank strings are treated as missing.
- Issue number and writer are preferred for subtitle generation.
- Penciller may be used when writer is unavailable or when a compact display has room.

## LibraryGroup

Represents one selectable group in the library filter control.

**Fields**:
- `kind`: `Series` or `Folder`.
- `key`: normalized group key used for matching.
- `label`: user-facing display name.
- `item_count`: number of library items in the group.

**Validation Rules**:
- Groups are sorted by label for predictable browsing.
- Groups with empty labels are omitted.
- Series grouping normalizes casing and surrounding whitespace.
- Folder grouping uses a stable key so duplicate folder labels do not merge unrelated paths.

## ActiveLibraryFilter

Represents the current library filter selection.

**Fields**:
- `kind`: selected group kind.
- `key`: selected normalized group key.

**State Transitions**:
- `None` -> `Some(filter)`: user selects a series or folder.
- `Some(filter)` -> `None`: user clears the filter.
- `Some(filter)` -> `Some(other_filter)`: user selects another group.
- `Some(filter)` -> `None`: library refresh removes the selected group.

## Relationships

- `LibraryGridItem` may have zero or one metadata record.
- `LibraryGridItem` may belong to zero or one series group.
- `LibraryGridItem` should belong to a folder group when its source path has a parent directory.
- `ActiveLibraryFilter` matches `LibraryGridItem` by group kind and key.
