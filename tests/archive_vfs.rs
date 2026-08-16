use std::io::Write;

use cbr_egui::vfs::{
    ArchiveError, ArchiveReader, PdfArchiveReader, RarArchiveReader, ZipArchiveReader, build_pages,
    is_page_image_path, read_page_bytes,
};

#[test]
fn natural_page_order_uses_numeric_values() {
    let pages = build_pages(["page_10.jpg", "page_1.jpg", "page_2.jpg"]);
    let paths = pages.into_iter().map(|page| page.path).collect::<Vec<_>>();

    assert_eq!(paths, ["page_1.jpg", "page_2.jpg", "page_10.jpg"]);
}

#[test]
fn filters_hidden_metadata_and_non_images() {
    assert!(is_page_image_path("chapter/page_1.jpg"));
    assert!(!is_page_image_path("__MACOSX/page_1.jpg"));
    assert!(!is_page_image_path("chapter/notes.txt"));
}

#[test]
fn zip_lists_pages_in_natural_order() {
    let archive_path = zip_fixture();
    let mut reader = ZipArchiveReader::new(archive_path.path());

    let pages = reader.list_pages().expect("list pages");
    let paths = pages.into_iter().map(|page| page.path).collect::<Vec<_>>();

    assert_eq!(paths, ["page_1.jpg", "page_2.jpg", "page_10.jpg"]);
}

#[test]
fn zip_reads_requested_page_bytes() {
    let archive_path = zip_fixture();
    let mut reader = ZipArchiveReader::new(archive_path.path());

    let bytes = reader.read_page("page_2.jpg").expect("read page");

    assert_eq!(bytes, b"two");
}

#[test]
fn zip_missing_page_returns_recoverable_error() {
    let archive_path = zip_fixture();
    let mut reader = ZipArchiveReader::new(archive_path.path());
    let error = reader.read_page("missing.jpg").expect_err("missing page");

    assert!(matches!(error, ArchiveError::NotFound(path) if path == "missing.jpg"));
}

#[test]
fn rar_reader_reports_backend_failure_recoverably() {
    let mut reader = RarArchiveReader::new("missing.cbr");
    let error = reader.list_pages().expect_err("rar backend");

    assert!(matches!(
        error,
        ArchiveError::BackendUnavailable(_) | ArchiveError::Read(_) | ArchiveError::Io(_)
    ));
}

#[test]
fn pdf_reader_reports_missing_runtime_recoverably() {
    let mut reader = PdfArchiveReader::new("missing.pdf");
    let error = reader.list_pages().expect_err("pdf backend or file error");

    assert!(matches!(
        error,
        ArchiveError::BackendUnavailable(_) | ArchiveError::Read(_) | ArchiveError::Io(_)
    ));
}

#[test]
fn cached_reader_serves_repeated_and_alternating_archives() {
    // read_page_bytes keeps the last reader per thread so the zip central
    // directory is not reparsed per page and the rar cursor survives. The
    // cache is keyed by path, so it must not serve one archive's bytes for
    // another, and must stay correct when a path is revisited.
    let dir = tempfile::tempdir().expect("dir");
    let first = dir.path().join("first.cbz");
    let second = dir.path().join("second.cbz");
    write_zip_fixture(&first);
    write_zip_fixture(&second);

    for _ in 0..3 {
        assert_eq!(
            read_page_bytes(&first, "page_1.jpg").expect("first archive"),
            b"one"
        );
        assert_eq!(
            read_page_bytes(&first, "page_2.jpg").expect("same archive again"),
            b"two"
        );
        assert_eq!(
            read_page_bytes(&second, "page_10.jpg").expect("second archive"),
            b"ten"
        );
    }
}

fn write_zip_fixture(path: &std::path::Path) {
    let file = std::fs::File::create(path).expect("zip file");
    write_zip_entries(file);
}

fn zip_fixture() -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().expect("zip file");
    write_zip_entries(file.reopen().expect("reopen fixture"));
    file
}

fn write_zip_entries(file: std::fs::File) {
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("page_10.jpg", options).expect("page 10");
    zip.write_all(b"ten").expect("page 10 bytes");
    zip.start_file("page_1.jpg", options).expect("page 1");
    zip.write_all(b"one").expect("page 1 bytes");
    zip.start_file("page_2.jpg", options).expect("page 2");
    zip.write_all(b"two").expect("page 2 bytes");
    zip.start_file("__MACOSX/page_3.jpg", options)
        .expect("hidden");
    zip.write_all(b"hidden").expect("hidden bytes");
    zip.start_file("notes.txt", options).expect("notes");
    zip.write_all(b"notes").expect("notes bytes");
    zip.finish().expect("finish zip");
}
