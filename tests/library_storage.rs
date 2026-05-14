use cbr_egui::library::{ComicAvailability, ComicInput, LibraryService};

fn service() -> (tempfile::TempDir, LibraryService) {
    let dir = tempfile::tempdir().expect("temp database directory");
    let db_path = dir.path().join("library.sqlite");
    let service = LibraryService::initialize(&db_path).expect("initialize library");
    (dir, service)
}

#[test]
fn initializes_required_schema() {
    let (_dir, service) = service();

    for table in ["folders", "metadata", "comics", "progress"] {
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
