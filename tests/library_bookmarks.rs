use cbr_egui::library::{ComicInput, LibraryService};

fn service_with_comic() -> (tempfile::TempDir, LibraryService, i64) {
    let dir = tempfile::tempdir().expect("temp dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 100,
            metadata_id: None,
        })
        .expect("comic");
    (dir, service, comic.id)
}

#[test]
fn toggle_adds_then_removes_bookmark() {
    let (_dir, service, comic_id) = service_with_comic();

    assert!(service.toggle_bookmark(comic_id, 5).expect("toggle on"));
    assert!(service.is_bookmarked(comic_id, 5).expect("is bookmarked"));

    assert!(!service.toggle_bookmark(comic_id, 5).expect("toggle off"));
    assert!(!service.is_bookmarked(comic_id, 5).expect("not bookmarked"));
}

#[test]
fn list_bookmarks_returns_pages_in_order() {
    let (_dir, service, comic_id) = service_with_comic();

    for page in [9, 2, 40] {
        assert!(service.toggle_bookmark(comic_id, page).expect("toggle"));
    }

    let pages = service
        .list_bookmarks(comic_id)
        .expect("list")
        .into_iter()
        .map(|bookmark| bookmark.page_index)
        .collect::<Vec<_>>();
    assert_eq!(pages, [2, 9, 40]);
}

#[test]
fn toggling_on_records_exactly_one_bookmark() {
    let (_dir, service, comic_id) = service_with_comic();

    assert!(service.toggle_bookmark(comic_id, 7).expect("toggle on"));

    assert_eq!(service.list_bookmarks(comic_id).expect("list").len(), 1);
}

#[test]
fn removing_comic_cascades_bookmarks() {
    let (_dir, service, comic_id) = service_with_comic();
    service.toggle_bookmark(comic_id, 3).expect("toggle");

    service.remove_comic(comic_id).expect("remove comic");

    assert!(service.list_bookmarks(comic_id).expect("list").is_empty());
}
