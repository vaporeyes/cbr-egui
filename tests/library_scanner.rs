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
