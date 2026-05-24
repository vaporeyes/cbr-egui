use cbr_egui::library::{
    ArchivePage, MAX_THUMBNAIL_HEIGHT, ThumbnailCacheEntry, ThumbnailRequest, ThumbnailWorkerPool,
    cache_key_for_source, cache_path_for_source, cover_request_for_pages, is_thumbnail_stale,
    thumbnail_target_size,
};
use image::{ImageBuffer, ImageFormat, Rgba};
use std::io::Write;

#[test]
fn thumbnail_cache_key_is_stable_for_source_and_fingerprint() {
    assert_eq!(
        cache_key_for_source("/books/a.cbz", "123"),
        cache_key_for_source("/books/a.cbz", "123")
    );
    assert_ne!(
        cache_key_for_source("/books/a.cbz", "123"),
        cache_key_for_source("/books/a.cbz", "456")
    );
}

#[test]
fn thumbnail_source_invalidation_detects_stale_entries() {
    let dir = tempfile::tempdir().expect("dir");
    let cache_path = cache_path_for_source(dir.path(), "/books/a.cbz", "123");
    let entry = ThumbnailCacheEntry {
        source_path: "/books/a.cbz".to_owned(),
        source_fingerprint: "123".to_owned(),
        cache_path: cache_path.clone(),
    };

    assert!(is_thumbnail_stale(&entry, "123"));
    std::fs::write(&cache_path, b"png").expect("cache");
    assert!(!is_thumbnail_stale(&entry, "123"));
    assert!(is_thumbnail_stale(&entry, "456"));
}

#[test]
fn thumbnail_target_size_caps_height_to_300px() {
    assert_eq!(MAX_THUMBNAIL_HEIGHT, 300);
    assert_eq!(thumbnail_target_size(1200, 1800), [200, 300]);
    assert_eq!(thumbnail_target_size(200, 250), [200, 250]);
}

#[test]
fn cover_request_uses_first_usable_page() {
    let pages = vec![
        ArchivePage {
            path: "page_2.jpg".to_owned(),
            sort_index: 1,
        },
        ArchivePage {
            path: "page_1.jpg".to_owned(),
            sort_index: 0,
        },
    ];

    assert_eq!(
        cover_request_for_pages(&pages).expect("cover").path,
        "page_1.jpg"
    );
    assert!(cover_request_for_pages(&[]).is_none());
}

#[test]
fn thumbnail_worker_generates_cache_file_in_background() {
    let dir = tempfile::tempdir().expect("dir");
    let archive_path = dir.path().join("book.cbz");
    let cache_path = dir.path().join("thumb.png");
    write_cbz_with_png(&archive_path, 40, 80);
    let pool = ThumbnailWorkerPool::start(1, 4).expect("pool");
    pool.submit(ThumbnailRequest {
        source_path: archive_path.to_string_lossy().into_owned(),
        source_fingerprint: "fingerprint".to_owned(),
        cache_path: cache_path.clone(),
    })
    .expect("submit");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let result = loop {
        if let Some(result) = pool.try_recv() {
            break result;
        }
        assert!(std::time::Instant::now() < deadline);
    };

    assert_eq!(result.cache_path, cache_path);
    assert_eq!(result.outcome.expect("thumbnail"), [40, 80]);
    assert!(cache_path.exists());
}

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = ImageBuffer::from_pixel(width, height, Rgba([255_u8, 0, 0, 255]));
    let mut cursor = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .expect("encode png");
    cursor.into_inner()
}

fn write_cbz_with_png(path: &std::path::Path, width: u32, height: u32) {
    let file = std::fs::File::create(path).expect("zip file");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("page_1.png", options).expect("page");
    zip.write_all(&png_bytes(width, height))
        .expect("page bytes");
    zip.finish().expect("finish zip");
}
