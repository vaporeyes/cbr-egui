// ABOUTME: Verifies resource limits, atomic persistence, and backend error handling.
// ABOUTME: Uses small malformed fixtures to test allocation guards without large allocations.
use std::io::Write;
use std::time::Duration;

use cbr_egui::app::ui::EguiComicReaderApp;
use cbr_egui::cache::PageTextureCache;
use cbr_egui::config::AppConfig;
use cbr_egui::decode::{
    DecodePurpose, DecodeRequest, DecodeRequestId, DecodeSource, ImageAdjustments, Rotation,
    WorkerPool,
};
use cbr_egui::library::{ComicInput, LibraryService, import_comic_file};
use cbr_egui::vfs::{ArchiveError, ArchiveReader, ZipArchiveReader};

fn zip_with_entry(path: &std::path::Path, name: &str, bytes: &[u8]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    zip.start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap();
}

#[test]
fn actual_exit_callback_flushes_settings_and_pending_progress() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).unwrap();
    let comic = service
        .upsert_comic(ComicInput {
            path: "book.cbz".into(),
            hash: "book".into(),
            page_count: 10,
            metadata_id: None,
        })
        .unwrap();
    let mut app = EguiComicReaderApp::with_config_and_service(
        AppConfig::default(),
        config_path.clone(),
        service,
    );
    app.set_dark_mode(false);
    app.inner.open_comic(comic.id, 10);
    app.inner.reading.as_mut().unwrap().set_current_page(4);
    eframe::App::on_exit(&mut app, None);
    assert!(!AppConfig::load(&config_path).dark_mode);
    assert_eq!(
        app.library_service
            .as_ref()
            .unwrap()
            .get_progress(comic.id)
            .unwrap()
            .unwrap()
            .current_page,
        4
    );
}

#[test]
fn decode_shutdown_disconnects_saturated_results_before_joining() {
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let pool = WorkerPool::start(1, 8).unwrap();
        for id in 0..8 {
            pool.submit(DecodeRequest {
                request_id: DecodeRequestId(id),
                page_index: 0,
                source: DecodeSource::Bytes(Vec::new()),
                purpose: DecodePurpose::Direct,
                target_size: None,
                rotation: Rotation::None,
                adjustments: ImageAdjustments::default(),
                cancellation_token: None,
            })
            .unwrap();
        }
        done_tx.send(pool.shutdown()).unwrap();
    });
    assert!(
        done_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .is_ok()
    );
}

#[test]
fn weighted_cache_evicts_by_bytes_and_rejects_oversized_entries() {
    let mut cache = PageTextureCache::with_byte_budget(8, 10, |size: &usize| *size).unwrap();
    cache.insert(1, 6);
    cache.insert(2, 4);
    cache.get(1);
    cache.insert(3, 4);
    assert!(cache.contains(1));
    assert!(!cache.contains(2));
    assert_eq!(cache.resident_bytes(), 10);
    assert_eq!(cache.insert(4, 11), Some(11));
    cache.clear();
    assert_eq!(cache.resident_bytes(), 0);
}

#[test]
fn zip_rejects_oversized_declared_entry_before_reading_payload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.cbz");
    zip_with_entry(&path, "page.jpg", b"small");
    let mut bytes = std::fs::read(&path).unwrap();
    let central = bytes.windows(4).position(|s| s == b"PK\x01\x02").unwrap();
    let size = (cbr_egui::vfs::limits::MAX_ENTRY_BYTES + 1) as u32;
    bytes[central + 24..central + 28].copy_from_slice(&size.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    let mut reader = ZipArchiveReader::new(&path);
    assert!(matches!(
        reader.read_page("page.jpg"),
        Err(ArchiveError::ResourceLimit(_))
    ));
}

#[test]
fn zip_metadata_has_a_smaller_decompression_budget() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.cbz");
    zip_with_entry(&path, "ComicInfo.xml", &vec![b' '; 1024 * 1024 + 1]);
    let mut reader = ZipArchiveReader::new(&path);
    assert!(matches!(
        reader.read_entry("ComicInfo.xml"),
        Err(ArchiveError::ResourceLimit(_))
    ));
}

#[test]
fn bounded_reads_and_rasters_enforce_limits() {
    use cbr_egui::vfs::limits::{MAX_RASTER_PIXELS, raster_size, read_bounded};
    assert!(read_bounded(&b"123456"[..], 5).is_err());
    assert!(raster_size(65_535, 65_535, None).is_err());
    let [width, height] = raster_size(8000, 8000, None).unwrap();
    assert!(u64::from(width) * u64::from(height) <= MAX_RASTER_PIXELS);
    assert_eq!(
        raster_size(1000, 2000, Some([200, 300])).unwrap(),
        [150, 300]
    );
    assert_eq!(
        cbr_egui::library::thumbnail_target_size(60_000, 100),
        [600, 1]
    );
}

#[test]
fn explicit_zip_import_survives_a_managed_store_rescan() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("book.zip");
    zip_with_entry(&source, "page.jpg", b"page");
    let store = dir.path().join("store");
    let imported = import_comic_file(&source, &store).unwrap();
    assert_eq!(imported.stored_path.extension().unwrap(), "cbz");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).unwrap();
    let comic = service.persist_imported_comic(&imported).unwrap();
    service.save_progress(comic.id, 0, false).unwrap();
    service
        .reconcile_scanned_comics(&cbr_egui::library::scan_library_root(&store).unwrap())
        .unwrap();
    assert_eq!(service.list_comics().unwrap().len(), 1);
    assert!(service.get_progress(comic.id).unwrap().is_some());
    assert_eq!(
        service.library_grid_items().unwrap()[0]
            .folder_label
            .as_deref(),
        dir.path().file_name().unwrap().to_str()
    );
}

#[test]
fn reimport_preserves_existing_bookmarks_and_progress() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("book.cbz");
    zip_with_entry(&source, "page.jpg", b"page");
    let store = dir.path().join("store");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).unwrap();
    let comic = service
        .persist_imported_comic(&import_comic_file(&source, &store).unwrap())
        .unwrap();
    service.save_progress(comic.id, 0, true).unwrap();
    service.toggle_bookmark(comic.id, 0).unwrap();
    let renamed = dir.path().join("renamed.cbz");
    std::fs::copy(&source, &renamed).unwrap();
    let again = service
        .persist_imported_comic(&import_comic_file(&renamed, &store).unwrap())
        .unwrap();
    assert_eq!(comic.id, again.id);
    assert!(service.get_progress(again.id).unwrap().unwrap().is_read);
    assert!(service.is_bookmarked(again.id, 0).unwrap());
}

#[test]
fn atomic_configuration_replacement_leaves_complete_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let mut config = AppConfig::default();
    config.save(&path).unwrap();
    config.dark_mode = false;
    config.save(&path).unwrap();
    assert_eq!(AppConfig::load(&path), config);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn missing_managed_copy_is_relocated_without_losing_reading_state() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("book.cbz");
    zip_with_entry(&source, "page.jpg", b"page");
    let store = dir.path().join("store");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).unwrap();
    let imported = import_comic_file(&source, &store).unwrap();
    let comic = service.persist_imported_comic(&imported).unwrap();
    service.save_progress(comic.id, 0, true).unwrap();
    service.toggle_bookmark(comic.id, 0).unwrap();
    std::fs::remove_file(&imported.stored_path).unwrap();
    let renamed = dir.path().join("renamed.cbz");
    std::fs::rename(&source, &renamed).unwrap();
    let repaired = service
        .persist_imported_comic(&import_comic_file(&renamed, &store).unwrap())
        .unwrap();
    assert_eq!(repaired.id, comic.id);
    assert!(std::path::Path::new(&repaired.path).is_file());
    assert!(service.is_bookmarked(comic.id, 0).unwrap());
    assert!(service.get_progress(comic.id).unwrap().unwrap().is_read);
}

#[test]
fn batch_import_rolls_back_if_any_database_write_fails() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("book.cbz");
    zip_with_entry(&source, "page.jpg", b"page");
    let first = import_comic_file(&source, &dir.path().join("store")).unwrap();
    let mut second = first.clone();
    second.stored_path = dir.path().join("reject.cbz");
    second.content_hash = "reject".into();
    let db = dir.path().join("library.sqlite");
    let service = LibraryService::initialize(&db).unwrap();
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_comic BEFORE INSERT ON comics WHEN NEW.hash = 'reject' BEGIN SELECT RAISE(ABORT, 'test rejection'); END;").unwrap();
    assert!(service.persist_import_batch(&[first, second]).is_err());
    assert!(service.list_comics().unwrap().is_empty());
}

#[test]
fn existing_schema_migrates_without_losing_progress_or_inventing_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite");
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection.execute_batch("CREATE TABLE comics (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, hash TEXT NOT NULL, page_count INTEGER NOT NULL, metadata_id INTEGER NULL);
        INSERT INTO comics VALUES (42, '/store/abc/book.cbz', 'abc', 10, NULL);
        CREATE TABLE progress (comic_id INTEGER PRIMARY KEY, current_page INTEGER NOT NULL, is_read INTEGER NOT NULL);
        INSERT INTO progress VALUES (42, 5, 0);").unwrap();
    drop(connection);
    let service = LibraryService::initialize(&db).unwrap();
    assert_eq!(service.get_progress(42).unwrap().unwrap().current_page, 5);
    assert!(
        service.library_grid_items().unwrap()[0]
            .folder_label
            .is_none()
    );
    assert!(
        service
            .get_comic_row(42)
            .unwrap()
            .unwrap()
            .source_path
            .is_none()
    );
}

#[test]
fn djvu_viewing_returns_pixels_at_requested_render_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.djvu");
    let pages = vec![djvu_rs::Pixmap::white(64, 96)];
    let bytes = djvu_rs::djvu_encode::encode_djvm_layered_shared(
        &pages,
        djvu_rs::djvu_encode::EncodeQuality::Quality,
        300,
        None,
        2,
    )
    .unwrap();
    std::fs::write(&path, bytes).unwrap();
    let data = cbr_egui::vfs::read_page_data(&path, 0, Some([20, 30])).unwrap();
    let cbr_egui::vfs::PageData::Pixels(image) = data else {
        panic!("document viewing must return pixels")
    };
    assert_eq!([image.width(), image.height()], [20, 30]);
}

#[test]
fn oversized_djvu_input_is_rejected_before_parsing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.djvu");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(cbr_egui::vfs::limits::MAX_DOCUMENT_BYTES + 1)
        .unwrap();
    let mut reader = cbr_egui::vfs::DjvuArchiveReader::new(&path);
    assert!(matches!(
        reader.list_pages(),
        Err(ArchiveError::ResourceLimit(_))
    ));
}

#[cfg(unix)]
#[test]
fn zip_reader_keeps_the_opened_archive_between_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.cbz");
    zip_with_entry(&path, "page.jpg", b"original");
    let mut reader = ZipArchiveReader::new(&path);
    reader.list_pages().unwrap();
    std::fs::rename(&path, dir.path().join("previous.cbz")).unwrap();
    zip_with_entry(&path, "page.jpg", b"replacement");
    assert_eq!(reader.read_page("page.jpg").unwrap(), b"original");
}

#[test]
fn saturated_decode_queue_defers_the_page_without_opening_it_on_the_ui_thread() {
    let dir = tempfile::tempdir().unwrap();
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).unwrap();
    let comic = service
        .upsert_comic(ComicInput {
            path: dir
                .path()
                .join("missing.cbz")
                .to_string_lossy()
                .into_owned(),
            hash: "fixture".into(),
            page_count: 1,
            metadata_id: None,
        })
        .unwrap();
    let item = service.library_grid_items().unwrap().remove(0);
    let pool = WorkerPool::start(1, 1).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    // One queued result, one blocked sender, and one pending request keep the
    // request queue full until the test drops the receiver.
    for id in 0..3 {
        let request = DecodeRequest {
            request_id: DecodeRequestId(id),
            page_index: 0,
            source: DecodeSource::Bytes(Vec::new()),
            purpose: DecodePurpose::Direct,
            target_size: None,
            rotation: Rotation::None,
            adjustments: ImageAdjustments::default(),
            cancellation_token: None,
        };
        while pool.submit(request.clone()).is_err() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
    let mut app = cbr_egui::app::ComicReaderApp::<eframe::egui::TextureHandle>::default();
    app.open_comic(comic.id, 1);
    app.reading.as_mut().unwrap().decode_worker_pool = Some(pool);
    cbr_egui::app::reader::load_reader_page(&eframe::egui::Context::default(), &mut app, &item, 0);
    let session = app.reading.as_ref().unwrap();
    assert!(session.viewer_state.page_status.is_empty());
    assert_eq!(session.archive_cache.page_count(), 0);
    assert!(session.prefetch.in_flight.is_empty());
}

#[test]
fn rar_enumeration_reports_an_invalid_file_header() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.cbr");
    // RAR4 signature and empty main header, followed by a file header with an
    // invalid CRC. Opening succeeds; enumeration must report the corruption.
    let mut bytes = vec![
        0x52, 0x61, 0x72, 0x21, 0x1a, 0x07, 0x00, 0xcf, 0x90, 0x73, 0x00, 0x00, 0x0d, 0x00, 0, 0,
        0, 0, 0, 0,
    ];
    let mut header = [0u8; 40];
    header[2] = 0x74;
    header[5] = 40;
    bytes.extend_from_slice(&header);
    std::fs::write(&path, bytes).unwrap();
    assert!(unrar::Archive::new(&path).open_for_listing().is_ok());
    let mut reader = cbr_egui::vfs::RarArchiveReader::new(&path);
    assert!(matches!(reader.list_pages(), Err(ArchiveError::Read(_))));
}
