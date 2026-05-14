# Research: Domain Models & Data Layer

## Decision: Keep library persistence behind LibraryService-owned SQLite modules

**Rationale**: The constitution requires LibraryService to own metadata, SQLite
access, progress, and future bookmarks. `rusqlite` fits the current single
desktop binary, supports deterministic migrations and tests, and avoids adding
an async runtime that the project does not need for local storage.

**Alternatives considered**: Direct SQLite calls from callers were rejected
because they blur the LibraryService boundary. An async database layer was
rejected because this feature has no network/server runtime need.

## Decision: Model progress as one row per comic with upsert semantics

**Rationale**: FR-004 requires creating, reading, and updating progress without
duplicates. A `progress.comic_id` primary key or unique constraint makes the
invariant enforceable in storage rather than relying on caller discipline.

**Alternatives considered**: Append-only progress events were rejected because
the current feature only needs current reading state and duplicate prevention.

## Decision: Parse ComicInfo.xml with `quick-xml` and serde-compatible structs

**Rationale**: ComicInfo.xml is a small XML document with known optional fields.
`quick-xml` provides maintained streaming XML support and serde integration,
allowing absent fields to deserialize cleanly while malformed XML returns a
recoverable metadata error.

**Alternatives considered**: Manual string parsing was rejected because casing,
escaping, and malformed XML handling are easy to get wrong. A DOM-heavy XML
stack was rejected as unnecessary for four target fields.

## Decision: Use `zip` for ZIP/CBZ page enumeration and byte reads

**Rationale**: The `zip` crate is the standard maintained Rust option for ZIP
archives and supports listing entries and opening one file at a time. That meets
FR-009 and FR-013 without extracting the whole archive.

**Alternatives considered**: Shelling out to unzip was rejected because it makes
portable error handling and on-demand reads harder.

## Decision: Implement RAR/CBR behind the same ArchiveReader trait with a `7z` backend

**Rationale**: Keeping RAR code behind `ArchiveReader` lets backend-specific
limitations stay isolated while preserving the shared page listing and byte
retrieval contract. The implementation uses `7z` to list archive entries and
stream requested entries to stdout without normal full-archive extraction; if
`7z` is missing, callers receive a recoverable backend-unavailable error.

**Alternatives considered**: Full extraction to a temp directory was rejected by
the archive-native VFS principle. Deferring RAR entirely was rejected because
FR-010 requires RAR support in this feature. `compress-tools` or `unrar` FFI can
replace the command backend later without changing the public trait.

## Decision: Centralize image filtering and natural page ordering in `vfs::ordering`

**Rationale**: ZIP and RAR readers need identical behavior for hidden metadata
directories, non-image filtering, and natural ordering. A shared ordering module
keeps acceptance tests backend-independent and avoids drift.

**Alternatives considered**: Per-reader sorting was rejected because it would
duplicate edge-case logic. Plain lexicographic sorting was rejected because it
places `page_10` before `page_2`.

## Decision: Return typed recoverable errors for metadata and archive failures

**Rationale**: The spec requires missing metadata to be non-fatal, malformed
metadata to be recoverable, and bad pages to avoid panics. A `thiserror`-based
error enum lets callers distinguish absent metadata, parse errors, missing page
paths, corrupt archives, and unsupported entries.

**Alternatives considered**: `anyhow` everywhere was rejected for public module
contracts because callers need structured recovery. Panics were rejected by the
constitution.
