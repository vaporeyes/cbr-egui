# cbr-egui

A native CBR / CBZ / PDF comic reader built in Rust with [egui].

![cbr-egui](assets/cbr-egui.png)

## Features

- **Library**: import individual files or whole folders; comics are copied
  into a content-addressed managed store so the library remains valid even
  if the originals move. Filter by series or folder, switch between
  thumbnail and list views, and pull up a collapsible shelf for quick
  navigation.
- **Reader**: paged or continuous-vertical layout, two-page spread, fit /
  fill / width / height / 1:1 zoom presets, smooth zoom + pan, rotation,
  bookmarks, and per-page progress tracking.
- **Image adjustments**: brightness, contrast, gamma, grayscale mix, and a
  value-study mode that posterizes luma into 2–8 bands.
- **Familiar menu bar**: File / Tools / Options / Help in both the library
  and reader views.
- **Right-click any comic** for "Open file location" to reveal the source
  file in the OS file manager.
- Resumes the last reading session on launch (toggle in Settings).

## Supported formats

| Extension | Backend |
|-----------|---------|
| `.cbz` / `.zip` | [`zip`] |
| `.cbr` / `.rar` | [`unrar`] |
| `.pdf` | [`pdfium-render`] |

## Building

Requires a recent stable Rust toolchain (2024 edition).

```sh
cargo run --release
```

PDF rendering requires the `pdfium` shared library to be available at
runtime — see the [pdfium-render docs][pdfium-render] for platform setup.

## Tests

```sh
cargo test
```

## Project layout

- `src/app/` — application shell, library view, reader routing.
- `src/viewer/` — viewport state, layout math, page rendering.
- `src/decode/` — image decode pipeline (rotation, adjustments, worker pool).
- `src/library/` — managed store, import, SQLite-backed metadata.
- `src/cache/` — page texture cache.
- `src/vfs/` — archive readers (zip / rar / pdf).
- `tests/` — integration tests.

[egui]: https://github.com/emilk/egui
[`zip`]: https://crates.io/crates/zip
[`unrar`]: https://crates.io/crates/unrar
[pdfium-render]: https://crates.io/crates/pdfium-render
