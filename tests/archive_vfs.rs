use std::io::Write;

use cbr_egui::vfs::{
    ArchiveError, ArchiveReader, DjvuArchiveReader, PdfArchiveReader, RarArchiveReader,
    ZipArchiveReader, build_pages, is_page_image_path, read_page_bytes,
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
fn djvu_lists_every_page_and_renders_them_as_images() {
    let dir = tempfile::tempdir().expect("dir");
    let book = dir.path().join("book.djvu");
    write_djvu_fixture(&book, &[(64, 96), (80, 100), (48, 72)]);
    let mut reader = DjvuArchiveReader::new(&book);

    let pages = reader.list_pages().expect("list pages");
    let paths = pages
        .iter()
        .map(|page| page.path.clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["page_1.png", "page_2.png", "page_3.png"]);

    // Each page must come back as decodable image bytes at its own size, which
    // is what the decode pipeline expects from every reader.
    for (index, expected) in [(0, (64, 96)), (1, (80, 100)), (2, (48, 72))] {
        let bytes = reader.read_page(&paths[index]).expect("read page");
        let image = image::load_from_memory(&bytes).expect("decode rendered page");
        assert_eq!((image.width(), image.height()), expected);
    }
}

#[test]
fn djvu_is_reachable_through_extension_dispatch() {
    let dir = tempfile::tempdir().expect("dir");
    // Both the long name and the historical short form are DjVu.
    for name in ["book.djvu", "book.djv", "BOOK.DJVU"] {
        let path = dir.path().join(name);
        write_djvu_fixture(&path, &[(32, 48)]);

        let bytes = read_page_bytes(&path, "page_1.png").expect("read through dispatch");
        let image = image::load_from_memory(&bytes).expect("decode");
        assert_eq!((image.width(), image.height()), (32, 48));
    }
}

#[test]
fn djvu_missing_and_non_page_entries_are_distinguished() {
    let dir = tempfile::tempdir().expect("dir");
    let book = dir.path().join("book.djvu");
    write_djvu_fixture(&book, &[(32, 48)]);
    let mut reader = DjvuArchiveReader::new(&book);

    // A name that is not a page at all is an absence, so metadata probing can
    // report "no ComicInfo.xml" rather than failing the import.
    assert!(reader.read_entry("ComicInfo.xml").expect("probe").is_none());
    // A page number past the end is a genuine miss.
    assert!(matches!(
        reader.read_page("page_9.png"),
        Err(ArchiveError::NotFound(_))
    ));
}

#[test]
fn corrupt_djvu_reports_a_recoverable_error() {
    let dir = tempfile::tempdir().expect("dir");
    let book = dir.path().join("book.djvu");
    std::fs::write(&book, b"this is not a djvu document").expect("write");
    let mut reader = DjvuArchiveReader::new(&book);

    assert!(matches!(
        reader.list_pages(),
        Err(ArchiveError::CorruptArchive(_))
    ));
}

/// Encodes a real multi-page DjVu bundle, one page per requested size.
fn write_djvu_fixture(path: &std::path::Path, sizes: &[(u32, u32)]) {
    let pages = sizes
        .iter()
        .enumerate()
        .map(|(index, &(width, height))| {
            let mut pixmap = djvu_rs::Pixmap::white(width, height);
            // Some ink so the encoder has foreground to segment.
            let shade = 30 + (index as u8) * 40;
            for y in height / 4..height / 2 {
                for x in width / 4..width / 2 {
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

#[test]
fn pdf_reports_non_page_entries_as_absent_not_failed() {
    let mut reader = PdfArchiveReader::new("missing.pdf");

    // Metadata discovery probes every archive for ComicInfo.xml. A PDF has no
    // such entry, which is an absence rather than an error, and answering that
    // must not require opening the document or the pdfium runtime.
    assert!(reader.read_entry("ComicInfo.xml").expect("probe").is_none());
    assert!(reader.read_entry("comicinfo.xml").expect("probe").is_none());
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

#[test]
fn djvu_reports_its_embedded_document_metadata() {
    let dir = tempfile::tempdir().expect("dir");
    let book = dir.path().join("titled.djvu");
    write_djvu_with_metadata(&book, "The Hunting of the Snark", "Lewis Carroll");
    let mut reader = DjvuArchiveReader::new(&book);

    let metadata = reader
        .document_metadata()
        .expect("metadata lookup")
        .expect("metadata present");

    assert_eq!(metadata.title.as_deref(), Some("The Hunting of the Snark"));
    assert_eq!(metadata.writer.as_deref(), Some("Lewis Carroll"));
}

#[test]
fn djvu_without_metadata_reports_none() {
    let dir = tempfile::tempdir().expect("dir");
    let book = dir.path().join("bare.djvu");
    write_djvu_fixture(&book, &[(32, 48)]);
    let mut reader = DjvuArchiveReader::new(&book);

    assert_eq!(reader.document_metadata().expect("metadata lookup"), None);
}

/// Encodes a single-page DjVu carrying a METz metadata chunk.
fn write_djvu_with_metadata(path: &std::path::Path, title: &str, author: &str) {
    let mut pixmap = djvu_rs::Pixmap::white(48, 64);
    for y in 16..32 {
        for x in 12..24 {
            pixmap.set_rgb(x, y, 40, 40, 40);
        }
    }
    let metadata = djvu_rs::metadata::DjVuMetadata {
        title: Some(title.to_owned()),
        author: Some(author.to_owned()),
        ..Default::default()
    };
    let bytes = djvu_rs::djvu_encode::PageEncoder::from_pixmap(&pixmap)
        .with_quality(djvu_rs::djvu_encode::EncodeQuality::Quality)
        .with_metadata(metadata)
        .encode()
        .expect("encode djvu with metadata");
    std::fs::write(path, bytes).expect("write djvu fixture");
}
