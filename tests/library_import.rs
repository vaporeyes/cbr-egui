use std::io::Write;
use std::path::Path;

use cbr_egui::library::{LibraryService, import_comic_file, import_paths};

fn cbz_fixture(path: &Path) {
    let file = std::fs::File::create(path).expect("zip file");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("page_1.jpg", options).expect("page 1");
    zip.write_all(b"one").expect("page 1 bytes");
    zip.start_file("page_2.jpg", options).expect("page 2");
    zip.write_all(b"two").expect("page 2 bytes");
    zip.finish().expect("finish zip");
}

fn cbz_fixture_with_metadata(path: &Path) {
    let file = std::fs::File::create(path).expect("zip file");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("ComicInfo.xml", options).expect("metadata");
    zip.write_all(
        br#"<ComicInfo><Series>Saga</Series><Title>Chapter One</Title><Number>12</Number><Writer>Brian K. Vaughan</Writer></ComicInfo>"#,
    )
    .expect("metadata bytes");
    zip.start_file("page_1.jpg", options).expect("page 1");
    zip.write_all(b"one").expect("page 1 bytes");
    zip.finish().expect("finish zip");
}

#[test]
fn import_copies_archive_into_hash_namespaced_store() {
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("My Book.cbz");
    cbz_fixture(&source);
    let store_root = dir.path().join("store");

    let imported = import_comic_file(&source, &store_root).expect("import");

    assert!(!imported.already_present);
    assert!(!imported.content_hash.is_empty());
    assert!(imported.stored_path.starts_with(&store_root));
    assert!(imported.stored_path.exists());
    assert_eq!(
        imported.stored_path.file_name().and_then(|n| n.to_str()),
        Some("My Book.cbz")
    );
    assert_eq!(
        imported.stored_path.parent().and_then(|p| p.file_name()),
        Some(std::ffi::OsStr::new(imported.content_hash.as_str()))
    );
    assert_eq!(imported.page_count, 2);
}

#[test]
fn reimporting_same_file_is_idempotent() {
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("book.cbz");
    cbz_fixture(&source);
    let store_root = dir.path().join("store");

    let first = import_comic_file(&source, &store_root).expect("first import");
    let second = import_comic_file(&source, &store_root).expect("second import");

    assert!(!first.already_present);
    assert!(second.already_present);
    assert_eq!(first.stored_path, second.stored_path);
    assert_eq!(first.content_hash, second.content_hash);

    let store_entries = std::fs::read_dir(&store_root)
        .expect("store dir")
        .count();
    assert_eq!(store_entries, 1);
}

#[test]
fn import_captures_comic_info_metadata() {
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("saga.cbz");
    cbz_fixture_with_metadata(&source);
    let store_root = dir.path().join("store");

    let imported = import_comic_file(&source, &store_root).expect("import");
    let metadata = imported.metadata.as_ref().expect("metadata");

    assert_eq!(metadata.series.as_deref(), Some("Saga"));
    assert_eq!(metadata.title.as_deref(), Some("Chapter One"));
    assert_eq!(metadata.number.as_deref(), Some("12"));
    assert_eq!(metadata.writer.as_deref(), Some("Brian K. Vaughan"));
}

#[test]
fn unsupported_files_are_rejected() {
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("notes.txt");
    std::fs::write(&source, b"not a comic").expect("notes");
    let store_root = dir.path().join("store");

    assert!(import_comic_file(&source, &store_root).is_err());
    assert!(!store_root.exists());
}

#[test]
fn import_paths_collects_per_file_failures() {
    let dir = tempfile::tempdir().expect("dir");
    let good = dir.path().join("good.cbz");
    cbz_fixture(&good);
    let bad = dir.path().join("bad.txt");
    std::fs::write(&bad, b"nope").expect("bad");
    let store_root = dir.path().join("store");

    let summary = import_paths(&[good.clone(), bad.clone()], &store_root);

    assert_eq!(summary.imported.len(), 1);
    assert_eq!(summary.failures.len(), 1);
    assert!(summary.imported[0].stored_path.exists());
    assert_eq!(summary.failures[0].0, bad);
}

#[test]
fn persisted_import_is_listable_in_library() {
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("saga.cbz");
    cbz_fixture_with_metadata(&source);
    let store_root = dir.path().join("store");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");

    let imported = import_comic_file(&source, &store_root).expect("import");
    let comic = service.persist_imported_comic(&imported).expect("persist");

    assert_eq!(comic.hash, imported.content_hash);
    assert_eq!(comic.page_count, 1);

    let items = service.library_grid_items().expect("grid items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "saga");
    assert_eq!(items[0].series.as_deref(), Some("Saga"));
}

#[test]
fn persisting_reimport_does_not_duplicate_comic() {
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("book.cbz");
    cbz_fixture(&source);
    let store_root = dir.path().join("store");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");

    let first = import_comic_file(&source, &store_root).expect("first import");
    let first_comic = service.persist_imported_comic(&first).expect("first persist");
    let second = import_comic_file(&source, &store_root).expect("second import");
    let second_comic = service.persist_imported_comic(&second).expect("second persist");

    assert_eq!(first_comic.id, second_comic.id);
    assert_eq!(service.list_comics().expect("list").len(), 1);
}

#[test]
fn djvu_books_import_into_the_managed_store() {
    let dir = tempfile::tempdir().expect("dir");
    let store = dir.path().join("store");
    let source = dir.path().join("book.djvu");
    write_djvu_fixture(&source, 4);

    let imported = import_comic_file(&source, &store).expect("import djvu");

    assert_eq!(imported.page_count, 4);
    assert!(imported.stored_path.starts_with(&store));
    assert!(imported.stored_path.exists());
    // A DjVu book carries no ComicInfo.xml. That is an absence, not a failure,
    // so the import still succeeds with no metadata.
    assert!(imported.metadata.is_none());
}

fn write_djvu_fixture(path: &Path, page_count: usize) {
    let pages = (0..page_count)
        .map(|index| {
            let mut pixmap = djvu_rs::Pixmap::white(48, 64);
            let shade = 30 + (index as u8) * 40;
            for y in 16..32 {
                for x in 12..24 {
                    pixmap.set_rgb(x, y, shade, shade, shade);
                }
            }
            pixmap
        })
        .collect::<Vec<_>>();

    let bytes = djvu_rs::djvu_encode::encode_djvm_layered_shared(
        &pages,
        djvu_rs::djvu_encode::EncodeQuality::Quality,
        300,
        None,
        2,
    )
    .expect("encode djvu fixture");
    std::fs::write(path, bytes).expect("write djvu fixture");
}
