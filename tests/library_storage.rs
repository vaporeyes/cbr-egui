use cbr_egui::library::{
    ComicAvailability, ComicInput, ComicMetadata, LibraryService, ScannedComic,
};

fn service() -> (tempfile::TempDir, LibraryService) {
    let dir = tempfile::tempdir().expect("temp database directory");
    let db_path = dir.path().join("library.sqlite");
    let service = LibraryService::initialize(&db_path).expect("initialize library");
    (dir, service)
}

fn metadata(service: &LibraryService, title: &str, number: &str, writer: &str) -> ComicMetadata {
    service
        .insert_metadata(&ComicMetadata {
            id: None,
            series: Some(title.to_owned()),
            title: Some(title.to_owned()),
            number: Some(number.to_owned()),
            writer: Some(writer.to_owned()),
            penciller: None,
        })
        .expect("metadata")
}

#[test]
fn initializes_required_schema() {
    let (_dir, service) = service();

    for table in ["folders", "metadata", "comics", "progress", "bookmarks"] {
        assert!(service.table_exists(table).expect("table lookup"));
    }
}

#[test]
fn creates_root_and_nested_folders() {
    let (_dir, service) = service();

    let root = service
        .create_folder("/library", None)
        .expect("root folder");
    let child = service
        .create_folder("/library/series", Some(root.id))
        .expect("child folder");
    let reloaded_child = service
        .get_folder(child.id)
        .expect("folder lookup")
        .expect("folder exists");

    assert_eq!(root.parent_id, None);
    assert_eq!(reloaded_child.parent_id, Some(root.id));
}

#[test]
fn upserts_comics_by_unique_path() {
    let (_dir, service) = service();

    let first = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash-a".to_owned(),
            page_count: 10,
            metadata_id: None,
        })
        .expect("insert comic");
    let updated = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash-b".to_owned(),
            page_count: 12,
            metadata_id: Some(42),
        })
        .expect("update comic");
    let reloaded = service
        .get_comic(first.id)
        .expect("comic lookup")
        .expect("comic exists");

    assert_eq!(first.id, updated.id);
    assert_eq!(reloaded.hash, "hash-b");
    assert_eq!(reloaded.page_count, 12);
    assert_eq!(reloaded.metadata_id, Some(42));
}

#[test]
fn upserts_progress_without_duplicates() {
    let (_dir, service) = service();
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash-a".to_owned(),
            page_count: 10,
            metadata_id: None,
        })
        .expect("comic");

    service
        .save_progress(comic.id, 3, false)
        .expect("initial progress");
    let progress = service
        .save_progress(comic.id, 9, true)
        .expect("updated progress");

    assert_eq!(progress.current_page, 9);
    assert!(progress.is_read);
    assert_eq!(
        service
            .progress_count_for_comic(comic.id)
            .expect("progress count"),
        1
    );
}

#[test]
fn persists_one_thousand_comics() {
    let (_dir, service) = service();

    for index in 0..1_000 {
        let comic = service
            .upsert_comic(ComicInput {
                path: format!("/library/book_{index}.cbz"),
                hash: format!("hash-{index}"),
                page_count: index + 1,
                metadata_id: None,
            })
            .expect("insert comic");
        service
            .save_progress(comic.id, index, index % 2 == 0)
            .expect("progress");
    }

    let last = service
        .upsert_comic(ComicInput {
            path: "/library/book_999.cbz".to_owned(),
            hash: "hash-999".to_owned(),
            page_count: 1_000,
            metadata_id: None,
        })
        .expect("read existing comic");
    let progress = service
        .get_progress(last.id)
        .expect("progress lookup")
        .expect("progress exists");

    assert_eq!(last.page_count, 1_000);
    assert_eq!(progress.current_page, 999);
}

#[test]
fn purges_unavailable_comics() {
    let (_dir, service) = service();
    service
        .upsert_comic(ComicInput {
            path: "/library/missing.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 1,
            metadata_id: None,
        })
        .expect("comic");
    service
        .set_comic_availability("/library/missing.cbz", ComicAvailability::Unavailable)
        .expect("unavailable");

    assert_eq!(service.purge_unavailable_comics().expect("purge"), 1);
    assert!(service.list_comics().expect("list").is_empty());
}

#[test]
fn remove_comic_deletes_row_progress_and_returns_comic() {
    let (_dir, service) = service();
    let meta = metadata(&service, "Saga", "1", "Brian K. Vaughan");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/Saga/001.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 10,
            metadata_id: meta.id,
        })
        .expect("comic");
    service.save_progress(comic.id, 4, false).expect("progress");

    let removed = service
        .remove_comic(comic.id)
        .expect("remove")
        .expect("was present");

    assert_eq!(removed.path, comic.path);
    assert!(service.get_comic(comic.id).expect("lookup").is_none());
    assert_eq!(
        service
            .progress_count_for_comic(comic.id)
            .expect("progress count"),
        0
    );
    assert!(service.list_comics().expect("list").is_empty());
}

#[test]
fn remove_comic_returns_none_for_unknown_id() {
    let (_dir, service) = service();

    assert!(service.remove_comic(999).expect("remove").is_none());
}

#[test]
fn resumes_last_read_available_comic() {
    let (_dir, service) = service();
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 20,
            metadata_id: None,
        })
        .expect("comic");
    service.save_progress(comic.id, 9, false).expect("progress");

    let (resumed, progress) = service.last_read_comic().expect("resume").expect("session");

    assert_eq!(resumed.id, comic.id);
    assert_eq!(progress.current_page, 9);
}

#[test]
fn resumes_most_recently_updated_progress_not_highest_page() {
    let (_dir, service) = service();
    let older = service
        .upsert_comic(ComicInput {
            path: "/library/older.cbz".to_owned(),
            hash: "older".to_owned(),
            page_count: 50,
            metadata_id: None,
        })
        .expect("older comic");
    let newer = service
        .upsert_comic(ComicInput {
            path: "/library/newer.cbz".to_owned(),
            hash: "newer".to_owned(),
            page_count: 50,
            metadata_id: None,
        })
        .expect("newer comic");

    service
        .save_progress(older.id, 40, false)
        .expect("older progress");
    std::thread::sleep(std::time::Duration::from_millis(5));
    service
        .save_progress(newer.id, 1, false)
        .expect("newer progress");

    let (resumed, progress) = service.last_read_comic().expect("resume").expect("session");

    assert_eq!(resumed.id, newer.id);
    assert_eq!(progress.current_page, 1);
}

#[test]
fn library_grid_items_left_join_metadata_without_hiding_unmatched_comics() {
    let (_dir, service) = service();
    let metadata = metadata(&service, "Saga", "12", "Brian K. Vaughan");
    service
        .upsert_comic(ComicInput {
            path: "/library/Saga/Saga 012.cbz".to_owned(),
            hash: "hash-a".to_owned(),
            page_count: 22,
            metadata_id: metadata.id,
        })
        .expect("comic with metadata");
    service
        .upsert_comic(ComicInput {
            path: "/library/Loose/No Metadata.cbz".to_owned(),
            hash: "hash-b".to_owned(),
            page_count: 10,
            metadata_id: None,
        })
        .expect("comic without metadata");

    let items = service.library_grid_items().expect("grid items");

    assert_eq!(items.len(), 2);
    let saga = items
        .iter()
        .find(|item| item.path.ends_with("Saga 012.cbz"))
        .expect("saga item");
    assert_eq!(
        saga.subtitle.as_deref(),
        Some("Issue #12 - Brian K. Vaughan")
    );
    assert_eq!(saga.series.as_deref(), Some("Saga"));
    assert_eq!(saga.series_key.as_deref(), Some("saga"));
    let loose = items
        .iter()
        .find(|item| item.path.ends_with("No Metadata.cbz"))
        .expect("loose item");
    assert_eq!(loose.subtitle, None);
    assert_eq!(loose.series_key, None);
    assert_eq!(loose.folder_label.as_deref(), Some("Loose"));
}

#[test]
fn reconciliation_reuses_metadata_row_for_existing_comic() {
    let (_dir, service) = service();
    let scanned = ScannedComic {
        path: "/library/Saga/Saga 001.cbz".to_owned(),
        fingerprint: "hash-a".to_owned(),
        page_count: 20,
        metadata: Some(ComicMetadata {
            id: None,
            series: Some("Saga".to_owned()),
            title: Some("Chapter One".to_owned()),
            number: Some("1".to_owned()),
            writer: Some("Brian K. Vaughan".to_owned()),
            penciller: None,
        }),
    };
    service
        .reconcile_scanned_comics(std::slice::from_ref(&scanned))
        .expect("first reconcile");
    let first_metadata_id = service
        .list_comics()
        .expect("list")
        .into_iter()
        .find(|comic| comic.path == scanned.path)
        .expect("first comic")
        .metadata_id
        .expect("first metadata");

    let updated_metadata = scanned.metadata.clone().expect("metadata");
    let updated = ScannedComic {
        fingerprint: "hash-b".to_owned(),
        metadata: Some(ComicMetadata {
            number: Some("2".to_owned()),
            ..updated_metadata
        }),
        ..scanned
    };
    service
        .reconcile_scanned_comics(std::slice::from_ref(&updated))
        .expect("second reconcile");
    let second_metadata_id = service
        .list_comics()
        .expect("list")
        .into_iter()
        .find(|comic| comic.path == updated.path)
        .expect("second comic")
        .metadata_id
        .expect("second metadata");
    let item = service
        .library_grid_items()
        .expect("grid")
        .into_iter()
        .find(|item| item.path == updated.path)
        .expect("grid item");

    assert_eq!(second_metadata_id, first_metadata_id);
    assert_eq!(
        item.subtitle.as_deref(),
        Some("Issue #2 - Brian K. Vaughan")
    );
}
