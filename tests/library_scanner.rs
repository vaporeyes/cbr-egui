use std::io::Write;

use cbr_egui::library::{
    ComicAvailability, ComicInput, LibraryService, discover_supported_archives,
    is_supported_archive_path, scan_library_root,
};

#[test]
fn discovers_supported_archives_recursively() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join("series")).expect("nested");
    std::fs::write(dir.path().join("book.cbz"), b"not zip").expect("book");
    std::fs::write(dir.path().join("series").join("book.cbr"), b"rar").expect("book");
    std::fs::write(dir.path().join("notes.txt"), b"notes").expect("notes");

    let archives = discover_supported_archives(dir.path()).expect("scan");
    let names = archives
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(names, ["book.cbz", "book.cbr"]);
}

#[test]
#[cfg(unix)]
fn symlink_loop_does_not_recurse_forever() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join("series")).expect("nested");
    std::fs::write(dir.path().join("series").join("book.cbz"), b"not zip").expect("book");
    // A link back to an ancestor. Path::is_dir follows it, so the walk used to
    // recurse until the stack was exhausted, aborting the process.
    std::os::unix::fs::symlink(dir.path(), dir.path().join("series").join("loop"))
        .expect("symlink");

    let archives = discover_supported_archives(dir.path()).expect("scan");

    assert_eq!(archives.len(), 1);
    assert!(archives[0].ends_with("book.cbz"));
}

#[test]
fn filters_hidden_metadata_and_unsupported_files() {
    assert!(is_supported_archive_path("book.cbz"));
    assert!(is_supported_archive_path("book.cbr"));
    assert!(is_supported_archive_path("book.pdf"));
    assert!(!is_supported_archive_path("book.txt"));

    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join("__MACOSX")).expect("hidden");
    std::fs::write(dir.path().join("__MACOSX").join("hidden.cbz"), b"hidden").expect("hidden");

    assert!(
        discover_supported_archives(dir.path())
            .expect("scan")
            .is_empty()
    );
}

#[test]
fn scan_reports_archives_and_page_counts_when_readable() {
    let dir = tempfile::tempdir().expect("dir");
    let archive = dir.path().join("book.cbz");
    zip_fixture(&archive);

    let scanned = scan_library_root(dir.path()).expect("scan");

    assert_eq!(scanned.len(), 1);
    assert_eq!(scanned[0].page_count, 2);
}

#[test]
fn scan_captures_comic_info_metadata_when_present() {
    let dir = tempfile::tempdir().expect("dir");
    let archive = dir.path().join("book.cbz");
    zip_fixture_with_metadata(&archive);

    let scanned = scan_library_root(dir.path()).expect("scan");

    let metadata = scanned[0].metadata.as_ref().expect("metadata");
    assert_eq!(metadata.series.as_deref(), Some("Saga"));
    assert_eq!(metadata.title.as_deref(), Some("Chapter One"));
    assert_eq!(metadata.number.as_deref(), Some("12"));
    assert_eq!(metadata.writer.as_deref(), Some("Brian K. Vaughan"));
}

#[test]
fn reconciliation_updates_removed_paths_without_duplicates() {
    let dir = tempfile::tempdir().expect("db");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let first = service
        .upsert_comic(ComicInput {
            path: "/library/old.cbz".to_owned(),
            hash: "old".to_owned(),
            page_count: 1,
            metadata_id: None,
        })
        .expect("old");

    service
        .reconcile_scanned_comics(&[cbr_egui::library::ScannedComic {
            path: "/library/new.cbz".to_owned(),
            fingerprint: "new".to_owned(),
            page_count: 2,
            metadata: None,
        }])
        .expect("reconcile");

    let old = service.get_comic(first.id).expect("old").expect("old");
    let comics = service.list_comics().expect("list");
    assert_eq!(old.availability, ComicAvailability::Unavailable);
    assert_eq!(comics.len(), 2);
}

fn zip_fixture(path: &std::path::Path) {
    let file = std::fs::File::create(path).expect("zip file");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("page_1.jpg", options).expect("page 1");
    zip.write_all(b"one").expect("page 1 bytes");
    zip.start_file("page_2.jpg", options).expect("page 2");
    zip.write_all(b"two").expect("page 2 bytes");
    zip.finish().expect("finish zip");
}

fn zip_fixture_with_metadata(path: &std::path::Path) {
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
fn reconciliation_preserves_the_content_hash_of_known_comics() {
    let dir = tempfile::tempdir().expect("db");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let imported = service
        .upsert_comic(ComicInput {
            path: "/store/abc123/book.cbz".to_owned(),
            hash: "blake3-content-hash".to_owned(),
            page_count: 10,
            metadata_id: None,
        })
        .expect("imported");

    // A rescan reports the cheap size:mtime fingerprint. That must not replace
    // the content hash, which names the thumbnail cache entry and the comic's
    // location in the managed store.
    service
        .reconcile_scanned_comics(&[cbr_egui::library::ScannedComic {
            path: "/store/abc123/book.cbz".to_owned(),
            fingerprint: "4096:1700000000".to_owned(),
            page_count: 10,
            metadata: None,
        }])
        .expect("reconcile");

    let reloaded = service
        .get_comic(imported.id)
        .expect("lookup")
        .expect("row");
    assert_eq!(reloaded.hash, "blake3-content-hash");
    assert_eq!(reloaded.availability, ComicAvailability::Available);
}

#[test]
fn djvu_books_are_discovered_and_report_their_page_count() {
    assert!(is_supported_archive_path("book.djvu"));
    assert!(is_supported_archive_path("book.djv"));
    assert!(is_supported_archive_path("BOOK.DJVU"));

    let dir = tempfile::tempdir().expect("dir");
    write_djvu_fixture(&dir.path().join("book.djvu"), 3);

    let scanned = scan_library_root(dir.path()).expect("scan");

    assert_eq!(scanned.len(), 1);
    assert_eq!(scanned[0].page_count, 3);
}

/// Encodes a real DjVu bundle with `page_count` pages.
fn write_djvu_fixture(path: &std::path::Path, page_count: usize) {
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
