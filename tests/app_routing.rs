use cbr_egui::app::ui::{
    EguiComicReaderApp, dispatch_continuous_prefetch_for_session, dispatch_prefetch_for_session,
    empty_library_text, load_reader_page, persist_scanned_comics_to_grid_items,
    read_archive_page_color_image, reconcile_prefetch_result, refresh_continuous_viewer_state,
    refresh_continuous_viewer_state_with_restore, responsive_grid_columns,
    scanned_comics_to_grid_items,
};
use cbr_egui::app::{AppState, CachedPage, ComicReaderApp, LibraryViewMode, ReadingSession};
use cbr_egui::config::AppConfig;
use cbr_egui::decode::{
    CancellationToken, DecodeError, DecodePurpose, DecodeRequestId, DecodeResult,
};
use cbr_egui::library::service::metadata_subtitle;
use cbr_egui::library::{
    ActiveLibraryFilter, ComicAvailability, ComicInput, ComicMetadataDisplay, LibraryGridItem,
    LibraryGroupKind, LibraryService, ScannedComic, ThumbnailStatus,
};
use cbr_egui::viewer::{
    ContinuousPageStatus, PageId, PageStatus, ReadingLayoutMode, Size2, VisiblePageWindow,
};
use eframe::egui;
use image::{ImageBuffer, ImageFormat, Rgba};
use std::io::{Cursor, Write};

fn resume_enabled_config() -> AppConfig {
    AppConfig {
        resume_last_session: true,
        ..AppConfig::default()
    }
}

fn grid_item(comic_id: i64, title: &str, path: &str) -> LibraryGridItem {
    LibraryGridItem {
        comic_id,
        title: title.to_owned(),
        subtitle: None,
        path: path.to_owned(),
        source_fingerprint: format!("fingerprint-{comic_id}"),
        page_count: 12,
        thumbnail_status: ThumbnailStatus::Missing,
        availability: ComicAvailability::Available,
        series: None,
        series_key: None,
        folder_label: std::path::Path::new(path)
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|value| value.to_str())
            .map(str::to_owned),
        folder_key: std::path::Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned()),
    }
}

fn series_item(comic_id: i64, title: &str, series: &str, path: &str) -> LibraryGridItem {
    let mut item = grid_item(comic_id, title, path);
    item.series = Some(series.to_owned());
    item.series_key = Some(
        series
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase(),
    );
    item.subtitle = Some("Issue #1 - Writer".to_owned());
    item
}

#[test]
fn responsive_grid_columns_never_drops_below_one() {
    assert_eq!(responsive_grid_columns(0.0, 190.0, 14.0), 1);
    assert_eq!(responsive_grid_columns(189.0, 190.0, 14.0), 1);
    assert_eq!(responsive_grid_columns(600.0, 190.0, 14.0), 3);
}

#[test]
fn app_state_routes_library_to_reading_and_back() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    let item = grid_item(42, "Book", "/library/book.cbz");

    assert!(app.open_grid_item(&item));
    assert_eq!(app.state, AppState::Reading(42));
    assert_eq!(app.reading.as_ref().expect("reading").page_count, 12);

    app.return_to_library();
    assert_eq!(app.state, AppState::Library);
    assert!(app.reading.is_none());
}

#[test]
fn library_view_mode_defaults_to_thumbnails_and_can_switch_to_list() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();

    assert_eq!(app.library.view_mode, LibraryViewMode::Thumbnails);

    app.library.view_mode = LibraryViewMode::List;

    assert_eq!(app.library.view_mode, LibraryViewMode::List);
}

#[test]
fn unavailable_comics_do_not_open_reader() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    let mut item = grid_item(42, "Book", "/library/book.cbz");
    item.availability = ComicAvailability::Unavailable;

    assert!(!app.open_grid_item(&item));
    assert_eq!(app.state, AppState::Library);
}

#[test]
fn empty_library_has_visible_startup_message() {
    let (title, detail) = empty_library_text();

    assert_eq!(title, "No library loaded");
    assert!(detail.contains("library root"));
}

#[test]
fn scanned_comics_become_available_grid_items() {
    let items = scanned_comics_to_grid_items(&[ScannedComic {
        path: "/library/My Book.cbz".to_owned(),
        fingerprint: "fingerprint".to_owned(),
        page_count: 24,
        metadata: None,
    }]);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "My Book");
    assert_eq!(items[0].page_count, 24);
    assert_eq!(items[0].availability, ComicAvailability::Available);
    assert_eq!(items[0].thumbnail_status, ThumbnailStatus::Missing);
    assert_eq!(items[0].subtitle, None);
    assert_eq!(items[0].folder_label.as_deref(), Some("library"));
}

#[test]
fn metadata_subtitle_omits_missing_fields_cleanly() {
    assert_eq!(
        metadata_subtitle(&ComicMetadataDisplay {
            number: Some("12".to_owned()),
            writer: Some("Brian K. Vaughan".to_owned()),
            ..ComicMetadataDisplay::default()
        })
        .as_deref(),
        Some("Issue #12 - Brian K. Vaughan")
    );
    assert_eq!(
        metadata_subtitle(&ComicMetadataDisplay {
            number: Some("12".to_owned()),
            ..ComicMetadataDisplay::default()
        })
        .as_deref(),
        Some("Issue #12")
    );
    assert_eq!(
        metadata_subtitle(&ComicMetadataDisplay {
            writer: Some("Brian K. Vaughan".to_owned()),
            ..ComicMetadataDisplay::default()
        })
        .as_deref(),
        Some("Brian K. Vaughan")
    );
    assert_eq!(
        metadata_subtitle(&ComicMetadataDisplay {
            penciller: Some("Fiona Staples".to_owned()),
            ..ComicMetadataDisplay::default()
        })
        .as_deref(),
        Some("Fiona Staples")
    );
    assert_eq!(metadata_subtitle(&ComicMetadataDisplay::default()), None);
}

#[test]
fn library_groups_normalize_series_names_and_count_items() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    app.library.items = vec![
        series_item(1, "Saga 1", "Saga", "/library/Saga/Saga 001.cbz"),
        series_item(2, "Saga 2", "  saga  ", "/library/Saga/Saga 002.cbz"),
        series_item(
            3,
            "Paper Girls 1",
            "Paper Girls",
            "/library/Paper Girls/001.cbz",
        ),
    ];
    app.library.refresh_filter_cache();

    let groups = app.library.groups();
    let saga = groups
        .iter()
        .find(|group| group.kind == LibraryGroupKind::Series && group.key == "saga")
        .expect("saga group");

    assert_eq!(saga.item_count, 2);
    assert_eq!(saga.label, "Saga");
}

#[test]
fn library_filter_uses_folder_fallback_for_items_without_series() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    app.library.items = vec![
        grid_item(1, "Loose 1", "/library/Loose/a.cbz"),
        grid_item(2, "Loose 2", "/library/Loose/b.cbz"),
        series_item(3, "Saga 1", "Saga", "/library/Saga/001.cbz"),
    ];
    app.library.active_filter = Some(ActiveLibraryFilter {
        kind: LibraryGroupKind::Folder,
        key: "/library/Loose".to_owned(),
    });
    app.library.refresh_filter_cache();

    let visible = app.library.visible_items().collect::<Vec<_>>();

    assert_eq!(visible.len(), 2);
    assert!(
        visible
            .iter()
            .all(|item| item.folder_label.as_deref() == Some("Loose"))
    );
}

#[test]
fn invalid_active_filter_is_cleared_after_library_refresh() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    app.library.items = vec![series_item(1, "Saga 1", "Saga", "/library/Saga/001.cbz")];
    app.library.active_filter = Some(ActiveLibraryFilter {
        kind: LibraryGroupKind::Series,
        key: "paper girls".to_owned(),
    });

    app.library.refresh_filter_cache();

    assert_eq!(app.library.active_filter, None);
}

#[test]
fn filtered_visible_item_opens_and_unavailable_filtered_item_is_blocked() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    let available = series_item(1, "Saga 1", "Saga", "/library/Saga/001.cbz");
    let mut unavailable = series_item(2, "Saga Missing", "Saga", "/library/Saga/missing.cbz");
    unavailable.availability = ComicAvailability::Unavailable;
    app.library.items = vec![available.clone(), unavailable.clone()];
    app.library.active_filter = Some(ActiveLibraryFilter {
        kind: LibraryGroupKind::Series,
        key: "saga".to_owned(),
    });
    app.library.refresh_filter_cache();

    let visible = app.library.visible_items().collect::<Vec<_>>();

    assert_eq!(visible.len(), 2);
    assert!(app.open_grid_item(&available));
    app.return_to_library();
    assert!(!app.open_grid_item(&unavailable));
    assert_eq!(
        app.library.status_text.as_deref(),
        Some("Comic is unavailable")
    );
}

#[test]
fn view_mode_switch_preserves_active_filter() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    app.library.active_filter = Some(ActiveLibraryFilter {
        kind: LibraryGroupKind::Series,
        key: "saga".to_owned(),
    });

    app.library.view_mode = LibraryViewMode::List;
    app.library.view_mode = LibraryViewMode::Thumbnails;

    assert_eq!(
        app.library.active_filter,
        Some(ActiveLibraryFilter {
            kind: LibraryGroupKind::Series,
            key: "saga".to_owned(),
        })
    );
}

#[test]
fn filtering_preserves_thumbnail_state_on_visible_items() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    let mut visible = series_item(1, "Saga 1", "Saga", "/library/Saga/001.cbz");
    visible.thumbnail_status = ThumbnailStatus::Loading;
    app.library.items = vec![
        visible,
        series_item(
            2,
            "Paper Girls 1",
            "Paper Girls",
            "/library/Paper Girls/001.cbz",
        ),
    ];
    app.library.active_filter = Some(ActiveLibraryFilter {
        kind: LibraryGroupKind::Series,
        key: "saga".to_owned(),
    });
    app.library.refresh_filter_cache();

    let visible = app.library.visible_items().collect::<Vec<_>>();

    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].thumbnail_status, ThumbnailStatus::Loading);
}

#[test]
fn persisted_scan_items_save_progress_with_real_comic_ids() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let items = persist_scanned_comics_to_grid_items(
        &service,
        &[ScannedComic {
            path: "/library/My Book.cbz".to_owned(),
            fingerprint: "fingerprint".to_owned(),
            page_count: 24,
            metadata: None,
        }],
    )
    .expect("persisted items");
    let item = items.first().expect("item").clone();
    let mut app = EguiComicReaderApp::with_config_and_service(
        AppConfig::default(),
        dir.path().join("config.json"),
        service,
    );

    assert!(app.inner.open_grid_item(&item));
    app.inner
        .reading
        .as_mut()
        .expect("reading")
        .set_current_page(3);
    app.flush_lifecycle_state().expect("save");

    let progress = app
        .library_service
        .as_ref()
        .expect("service")
        .get_progress(item.comic_id)
        .expect("progress lookup")
        .expect("progress");
    assert_eq!(progress.current_page, 3);
}

#[test]
fn active_progress_checkpoint_writes_without_shutdown_save_hook() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let items = persist_scanned_comics_to_grid_items(
        &service,
        &[ScannedComic {
            path: "/library/My Book.cbz".to_owned(),
            fingerprint: "fingerprint".to_owned(),
            page_count: 24,
            metadata: None,
        }],
    )
    .expect("persisted items");
    let item = items.first().expect("item").clone();
    let mut app = EguiComicReaderApp::with_config_and_service(
        AppConfig::default(),
        dir.path().join("config.json"),
        service,
    );

    assert!(app.inner.open_grid_item(&item));
    app.inner
        .reading
        .as_mut()
        .expect("reading")
        .set_current_page(5);
    app.checkpoint_active_progress().expect("checkpoint");

    assert!(app.config.resume_last_session);
    let progress = app
        .library_service
        .as_ref()
        .expect("service")
        .get_progress(item.comic_id)
        .expect("progress lookup")
        .expect("progress");
    assert_eq!(progress.current_page, 5);
}

#[test]
fn active_progress_checkpoint_skips_unchanged_progress() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 12,
            metadata_id: None,
        })
        .expect("comic");
    let mut app = EguiComicReaderApp::with_config_and_service(
        AppConfig::default(),
        dir.path().join("config.json"),
        service,
    );
    app.inner.open_comic(comic.id, comic.page_count as usize);
    app.inner
        .reading
        .as_mut()
        .expect("reading")
        .set_current_page(2);

    app.checkpoint_active_progress().expect("checkpoint");
    app.checkpoint_active_progress()
        .expect("unchanged checkpoint");

    assert_eq!(
        app.library_service
            .as_ref()
            .expect("service")
            .progress_count_for_comic(comic.id)
            .expect("count"),
        1
    );
}

#[test]
fn app_can_resume_last_read_session_from_library_service() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 12,
            metadata_id: None,
        })
        .expect("comic");
    service.save_progress(comic.id, 5, false).expect("progress");

    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();

    assert!(app.resume_last_session(&service).expect("resume"));
    assert_eq!(app.state, AppState::Reading(comic.id));
    assert_eq!(app.reading.expect("reading").current_page_index, 5);
}

#[test]
fn app_lifecycle_save_persists_active_reading_progress() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 12,
            metadata_id: None,
        })
        .expect("comic");
    let mut app = EguiComicReaderApp::with_config_and_service(
        AppConfig::default(),
        dir.path().join("config.json"),
        service,
    );
    app.inner.open_comic(comic.id, comic.page_count as usize);
    app.inner
        .reading
        .as_mut()
        .expect("reading")
        .set_current_page(4);

    app.flush_lifecycle_state().expect("save");

    let progress = app
        .library_service
        .as_ref()
        .expect("service")
        .get_progress(comic.id)
        .expect("progress lookup")
        .expect("progress");
    assert_eq!(progress.current_page, 4);
    assert!(!progress.is_read);
}

#[test]
fn app_lifecycle_save_marks_final_page_read() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 3,
            metadata_id: None,
        })
        .expect("comic");
    let mut app = EguiComicReaderApp::with_config_and_service(
        AppConfig::default(),
        dir.path().join("config.json"),
        service,
    );
    app.inner.open_comic(comic.id, comic.page_count as usize);
    app.inner
        .reading
        .as_mut()
        .expect("reading")
        .set_current_page(2);

    app.flush_lifecycle_state().expect("save");

    let progress = app
        .library_service
        .as_ref()
        .expect("service")
        .get_progress(comic.id)
        .expect("progress lookup")
        .expect("progress");
    assert!(progress.is_read);
}

#[test]
fn app_lifecycle_save_failure_is_non_fatal_status() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = EguiComicReaderApp::test_instance();
    app.config_path = dir.path().to_path_buf();

    assert!(app.flush_lifecycle_state().is_err());
    assert!(app.settings.last_error.is_some());
}

#[test]
fn startup_constructor_resumes_valid_saved_session() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 8,
            metadata_id: None,
        })
        .expect("comic");
    service.save_progress(comic.id, 6, false).expect("progress");

    let app = EguiComicReaderApp::with_config_and_service(
        resume_enabled_config(),
        dir.path().join("config.json"),
        service,
    );

    assert_eq!(app.inner.state, AppState::Reading(comic.id));
    assert_eq!(
        app.inner
            .reading
            .as_ref()
            .expect("reading")
            .current_page_index,
        6
    );
}

#[test]
fn startup_constructor_stays_library_when_resume_flag_is_disabled() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 8,
            metadata_id: None,
        })
        .expect("comic");
    service.save_progress(comic.id, 6, false).expect("progress");

    let app = EguiComicReaderApp::with_config_and_service(
        AppConfig::default(),
        dir.path().join("config.json"),
        service,
    );

    assert_eq!(app.inner.state, AppState::Library);
    assert!(app.inner.reading.is_none());
}

#[test]
fn startup_constructor_falls_back_without_saved_session() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");

    let app = EguiComicReaderApp::with_config_and_service(
        AppConfig::default(),
        dir.path().join("config.json"),
        service,
    );

    assert_eq!(app.inner.state, AppState::Library);
    assert!(app.inner.reading.is_none());
}

#[test]
fn startup_constructor_falls_back_for_unavailable_saved_comic() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/missing.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 8,
            metadata_id: None,
        })
        .expect("comic");
    service.save_progress(comic.id, 3, false).expect("progress");
    service
        .set_comic_availability(&comic.path, ComicAvailability::Unavailable)
        .expect("availability");

    let app = EguiComicReaderApp::with_config_and_service(
        resume_enabled_config(),
        dir.path().join("config.json"),
        service,
    );

    assert_eq!(app.inner.state, AppState::Library);
    assert!(app.inner.reading.is_none());
}

#[test]
fn startup_constructor_clamps_saved_page_to_page_count() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 3,
            metadata_id: None,
        })
        .expect("comic");
    service
        .save_progress(comic.id, 99, false)
        .expect("progress");

    let app = EguiComicReaderApp::with_config_and_service(
        resume_enabled_config(),
        dir.path().join("config.json"),
        service,
    );

    assert_eq!(
        app.inner
            .reading
            .as_ref()
            .expect("reading")
            .current_page_index,
        2
    );
}

#[test]
fn returning_to_library_disables_startup_resume() {
    let dir = tempfile::tempdir().expect("dir");
    let service = LibraryService::initialize(&dir.path().join("library.sqlite")).expect("service");
    let comic = service
        .upsert_comic(ComicInput {
            path: "/library/book.cbz".to_owned(),
            hash: "hash".to_owned(),
            page_count: 8,
            metadata_id: None,
        })
        .expect("comic");
    let config_path = dir.path().join("config.json");
    let mut app =
        EguiComicReaderApp::with_config_and_service(resume_enabled_config(), config_path, service);

    app.inner.open_comic(comic.id, comic.page_count as usize);
    let was_reading = matches!(app.inner.state, AppState::Reading(_));
    app.inner.return_to_library();
    app.reconcile_resume_state_after_route(was_reading);

    assert_eq!(app.inner.state, AppState::Library);
    assert!(!app.config.resume_last_session);
}

#[test]
fn settings_window_state_can_open_and_close() {
    let mut app = EguiComicReaderApp::test_instance();

    app.settings.open = true;
    assert!(app.settings.open);
    app.settings.open = false;
    assert!(!app.settings.open);
}

#[test]
fn dark_mode_preference_applies_to_context() {
    let ctx = egui::Context::default();
    let mut app = EguiComicReaderApp::test_instance();

    app.set_dark_mode(false);
    app.apply_config_to_context(&ctx);

    assert!(!ctx.style().visuals.dark_mode);
}

#[test]
fn app_can_purge_unavailable_items_from_library_view() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    app.library.items = vec![
        {
            let mut item = grid_item(1, "Available", "/library/a.cbz");
            item.page_count = 1;
            item
        },
        {
            let mut item = grid_item(2, "Missing", "/library/missing.cbz");
            item.page_count = 1;
            item.availability = ComicAvailability::Unavailable;
            item
        },
    ];

    assert_eq!(app.purge_unavailable_from_view(), 1);
    assert_eq!(app.library.items.len(), 1);
    assert_eq!(app.library.items[0].title, "Available");
}

#[test]
fn archive_page_loader_decodes_first_cbz_page_for_reader() {
    let dir = tempfile::tempdir().expect("dir");
    let archive_path = dir.path().join("book.cbz");
    write_cbz_with_png(&archive_path);

    let (image, pixel_size) =
        read_archive_page_color_image(&archive_path, 0).expect("decoded page");

    assert_eq!(image.size, [12, 18]);
    assert_eq!(pixel_size.width, 12.0);
    assert_eq!(pixel_size.height, 18.0);
}

#[test]
fn archive_page_loader_decodes_later_cbz_page_for_reader() {
    let dir = tempfile::tempdir().expect("dir");
    let archive_path = dir.path().join("book.cbz");
    write_cbz_with_png_pages(&archive_path, 5);

    let (image, pixel_size) =
        read_archive_page_color_image(&archive_path, 3).expect("decoded page");

    assert_eq!(image.size, [15, 21]);
    assert_eq!(pixel_size.width, 15.0);
    assert_eq!(pixel_size.height, 21.0);
}

#[test]
fn reading_session_initializes_prefetch_runtime_worker_and_bounded_cache() {
    let session = ReadingSession::<&str>::new(7, 12);

    assert_eq!(session.prefetch.generation.0, 0);
    assert!(session.prefetch.in_flight.is_empty());
    assert!(session.prefetch.queued_pages.is_empty());
    assert_eq!(session.texture_cache.capacity(), 5);
    assert!(session.decode_worker_pool.is_some());
}

#[test]
fn return_to_library_cancels_outstanding_prefetch_work() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    app.open_comic(42, 12);
    let token = CancellationToken::new();
    app.reading
        .as_mut()
        .expect("reading")
        .prefetch
        .track_in_flight(
            2,
            cbr_egui::decode::DecodeRequestId(1),
            DecodePurpose::Prefetch,
            token.clone(),
        );

    app.return_to_library();

    assert!(token.is_cancelled());
    assert_eq!(app.state, AppState::Library);
    assert!(app.reading.is_none());
}

#[test]
fn reading_session_cache_preserves_pixel_size_with_texture() {
    let mut session = ReadingSession::<&str>::new(7, 12);

    session.texture_cache.insert(
        3,
        cbr_egui::app::CachedPage {
            texture: "texture",
            pixel_size: Size2::new(100.0, 200.0),
        },
    );

    let cached = session.texture_cache.get(3).expect("cached");
    assert_eq!(cached.texture, "texture");
    assert_eq!(cached.pixel_size, Size2::new(100.0, 200.0));
}

#[test]
fn prefetch_dispatch_submits_next_following_and_previous_pages() {
    let dir = tempfile::tempdir().expect("dir");
    let archive_path = dir.path().join("book.cbz");
    write_cbz_with_png_pages(&archive_path, 6);
    let mut session = ReadingSession::<&str>::new(7, 6);
    session.set_current_page(2);

    let submitted =
        dispatch_prefetch_for_session(&mut session, archive_path.to_str().expect("archive path"));

    assert_eq!(submitted, 3);
    let mut in_flight = session
        .prefetch
        .in_flight
        .keys()
        .copied()
        .collect::<Vec<_>>();
    in_flight.sort_unstable();
    assert_eq!(in_flight, [1, 3, 4]);
}

#[test]
fn prefetch_reconciliation_inserts_successful_result_into_texture_cache() {
    let ctx = egui::Context::default();
    let mut session = ReadingSession::<egui::TextureHandle>::new(7, 6);
    let token = CancellationToken::new();
    session
        .prefetch
        .track_in_flight(2, DecodeRequestId(42), DecodePurpose::Prefetch, token);

    let inserted = reconcile_prefetch_result(
        &ctx,
        &mut session,
        DecodeResult {
            request_id: DecodeRequestId(42),
            page_index: 2,
            purpose: DecodePurpose::Prefetch,
            outcome: Ok(tiny_color_image([20, 30])),
        },
    );

    assert!(inserted);
    assert!(session.prefetch.in_flight.is_empty());
    let cached = session.texture_cache.get(2).expect("cached page");
    assert_eq!(cached.pixel_size, Size2::new(20.0, 30.0));
}

#[test]
fn load_reader_page_uses_prefetched_cache_before_archive_read() {
    let ctx = egui::Context::default();
    let mut app = ComicReaderApp::<egui::TextureHandle>::default();
    let mut item = grid_item(42, "Cached", "/missing/archive.cbz");
    item.page_count = 8;
    assert!(app.open_grid_item(&item));
    let texture = texture_for(&ctx, "cached-page", [16, 24]);
    app.reading.as_mut().expect("reading").texture_cache.insert(
        4,
        CachedPage {
            texture,
            pixel_size: Size2::new(16.0, 24.0),
        },
    );

    load_reader_page(&ctx, &mut app, &item, 4);

    let session = app.reading.as_ref().expect("reading");
    assert_eq!(session.current_page_index, 4);
    match &session.viewer_state.page_status {
        PageStatus::Ready {
            page_id,
            pixel_size,
            ..
        } => {
            assert_eq!(*page_id, PageId(4));
            assert_eq!(*pixel_size, Size2::new(16.0, 24.0));
        }
        _ => panic!("expected cached ready page"),
    }
}

#[test]
fn load_reader_page_cache_miss_uses_direct_decode_worker() {
    let ctx = egui::Context::default();
    let dir = tempfile::tempdir().expect("dir");
    let archive_path = dir.path().join("book.cbz");
    write_cbz_with_png_pages(&archive_path, 3);
    let mut app = ComicReaderApp::<egui::TextureHandle>::default();
    let mut item = grid_item(42, "Async", archive_path.to_str().expect("archive path"));
    item.page_count = 3;
    assert!(app.open_grid_item(&item));

    load_reader_page(&ctx, &mut app, &item, 1);

    let session = app.reading.as_mut().expect("reading");
    assert!(matches!(
        session.viewer_state.page_status,
        PageStatus::Loading { page_id: PageId(1) }
    ));
    let in_flight = session.prefetch.in_flight.get(&1).expect("direct request");
    assert_eq!(in_flight.purpose, DecodePurpose::Direct);
    assert_eq!(session.archive_cache.page_count(), 3);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let result = loop {
        if let Some(result) = session
            .decode_worker_pool
            .as_ref()
            .and_then(|pool| pool.try_recv())
        {
            break result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for direct decode"
        );
    };
    assert_eq!(result.purpose, DecodePurpose::Direct);
    assert!(reconcile_prefetch_result(&ctx, session, result));
    assert!(matches!(
        session.viewer_state.page_status,
        PageStatus::Ready {
            page_id: PageId(1),
            ..
        }
    ));
}

#[test]
fn failed_prefetch_reconciliation_is_recoverable() {
    let ctx = egui::Context::default();
    let mut session = ReadingSession::<egui::TextureHandle>::new(7, 6);
    session.viewer_state.set_ready(
        PageId(1),
        texture_for(&ctx, "visible-page", [8, 8]),
        Size2::new(8.0, 8.0),
    );
    session.prefetch.track_in_flight(
        3,
        DecodeRequestId(9),
        DecodePurpose::Prefetch,
        CancellationToken::new(),
    );

    let inserted = reconcile_prefetch_result(
        &ctx,
        &mut session,
        DecodeResult {
            request_id: DecodeRequestId(9),
            page_index: 3,
            purpose: DecodePurpose::Prefetch,
            outcome: Err(DecodeError::EmptyBytes),
        },
    );

    assert!(!inserted);
    assert!(session.prefetch.in_flight.is_empty());
    assert!(session.prefetch.failed_pages.contains_key(&3));
    assert!(matches!(
        session.viewer_state.page_status,
        PageStatus::Ready { .. }
    ));
}

#[test]
fn large_page_jump_cancels_stale_in_flight_prefetches() {
    let mut session = ReadingSession::<&str>::new(7, 60);
    session.set_current_page(2);
    let page_three = CancellationToken::new();
    let page_four = CancellationToken::new();
    session.prefetch.track_in_flight(
        3,
        DecodeRequestId(3),
        DecodePurpose::Prefetch,
        page_three.clone(),
    );
    session.prefetch.track_in_flight(
        4,
        DecodeRequestId(4),
        DecodePurpose::Prefetch,
        page_four.clone(),
    );

    session.set_current_page(50);
    let submitted = dispatch_prefetch_for_session(&mut session, "/missing/archive.cbz");

    assert_eq!(submitted, 0);
    assert!(page_three.is_cancelled());
    assert!(page_four.is_cancelled());
    assert!(session.prefetch.in_flight.is_empty());
}

#[test]
fn cancelled_prefetch_result_does_not_enter_texture_cache() {
    let ctx = egui::Context::default();
    let mut session = ReadingSession::<egui::TextureHandle>::new(7, 6);
    let token = CancellationToken::new();
    session.prefetch.track_in_flight(
        2,
        DecodeRequestId(88),
        DecodePurpose::Prefetch,
        token.clone(),
    );
    token.cancel();

    let inserted = reconcile_prefetch_result(
        &ctx,
        &mut session,
        DecodeResult {
            request_id: DecodeRequestId(88),
            page_index: 2,
            purpose: DecodePurpose::Prefetch,
            outcome: Ok(tiny_color_image([20, 30])),
        },
    );

    assert!(!inserted);
    assert!(!session.texture_cache.contains(2));
    assert!(session.prefetch.in_flight.is_empty());
}

#[test]
fn stale_generation_prefetch_result_does_not_enter_texture_cache() {
    let ctx = egui::Context::default();
    let mut session = ReadingSession::<egui::TextureHandle>::new(7, 6);
    session.prefetch.track_in_flight(
        2,
        DecodeRequestId(89),
        DecodePurpose::Prefetch,
        CancellationToken::new(),
    );
    session.set_current_page(4);

    let inserted = reconcile_prefetch_result(
        &ctx,
        &mut session,
        DecodeResult {
            request_id: DecodeRequestId(89),
            page_index: 2,
            purpose: DecodePurpose::Prefetch,
            outcome: Ok(tiny_color_image([20, 30])),
        },
    );

    assert!(!inserted);
    assert!(!session.texture_cache.contains(2));
    assert!(session.prefetch.in_flight.is_empty());
}

#[test]
fn opening_another_comic_cancels_existing_prefetch_work() {
    let mut app: ComicReaderApp<&str> = ComicReaderApp::default();
    app.open_comic(42, 12);
    let token = CancellationToken::new();
    app.reading
        .as_mut()
        .expect("reading")
        .prefetch
        .track_in_flight(
            2,
            DecodeRequestId(1),
            DecodePurpose::Prefetch,
            token.clone(),
        );

    app.open_comic(99, 5);

    assert!(token.is_cancelled());
    assert_eq!(app.state, AppState::Reading(99));
}

#[test]
fn continuous_dispatch_submits_only_visible_window_pages() {
    let dir = tempfile::tempdir().expect("dir");
    let archive_path = dir.path().join("book.cbz");
    write_cbz_with_png_sizes(
        &archive_path,
        &[(12, 18), (14, 20), (16, 22), (18, 24), (20, 26)],
    );
    let mut session = ReadingSession::<&str>::new(7, 5);

    let submitted = dispatch_continuous_prefetch_for_session(
        &mut session,
        archive_path.to_str().expect("archive path"),
        [0, 1, 2],
    );

    assert_eq!(submitted, 3);
    let mut in_flight = session
        .prefetch
        .in_flight
        .keys()
        .copied()
        .collect::<Vec<_>>();
    in_flight.sort_unstable();
    assert_eq!(in_flight, [0, 1, 2]);
}

#[test]
fn continuous_dispatch_skips_distant_pages_not_in_window() {
    let dir = tempfile::tempdir().expect("dir");
    let archive_path = dir.path().join("book.cbz");
    write_cbz_with_png_pages(&archive_path, 8);
    let mut session = ReadingSession::<&str>::new(7, 8);

    let submitted = dispatch_continuous_prefetch_for_session(
        &mut session,
        archive_path.to_str().expect("archive path"),
        [3, 4],
    );

    assert_eq!(submitted, 2);
    assert!(!session.prefetch.in_flight.contains_key(&0));
    assert!(!session.prefetch.in_flight.contains_key(&7));
}

#[test]
fn continuous_dispatch_cancels_stale_window_requests() {
    let dir = tempfile::tempdir().expect("dir");
    let archive_path = dir.path().join("book.cbz");
    write_cbz_with_png_pages(&archive_path, 8);
    let mut session = ReadingSession::<&str>::new(7, 8);
    let stale = CancellationToken::new();
    session.prefetch.track_in_flight(
        1,
        DecodeRequestId(1),
        DecodePurpose::Prefetch,
        stale.clone(),
    );

    let submitted = dispatch_continuous_prefetch_for_session(
        &mut session,
        archive_path.to_str().expect("archive path"),
        [5, 6],
    );

    assert_eq!(submitted, 2);
    assert!(stale.is_cancelled());
    assert!(!session.prefetch.in_flight.contains_key(&1));
    assert!(session.prefetch.in_flight.contains_key(&5));
    assert!(session.prefetch.in_flight.contains_key(&6));
}

#[test]
fn continuous_viewer_state_uses_cached_pages_for_virtual_canvas() {
    let ctx = egui::Context::default();
    let mut session = ReadingSession::<egui::TextureHandle>::new(7, 3);
    session
        .viewer_state
        .set_layout_mode(ReadingLayoutMode::ContinuousVertical);
    let texture = texture_for(&ctx, "continuous-cached-page", [20, 30]);
    session.texture_cache.insert(
        1,
        CachedPage {
            texture,
            pixel_size: Size2::new(20.0, 30.0),
        },
    );
    session
        .continuous_scroll
        .record_actual(1, Size2::new(20.0, 30.0));
    session.viewer_state.viewport_size = Size2::new(200.0, 300.0);

    refresh_continuous_viewer_state(&mut session);

    assert!(session.viewer_state.continuous_canvas.is_some());
    assert_eq!(session.viewer_state.continuous_pages.len(), 3);
    assert!(matches!(
        session.viewer_state.continuous_pages[1].status,
        ContinuousPageStatus::Ready { .. }
    ));
}

#[test]
fn continuous_fast_scroll_suppresses_anchor_restore_during_relayout() {
    let mut session = ReadingSession::<egui::TextureHandle>::new(7, 3);
    session
        .viewer_state
        .set_layout_mode(ReadingLayoutMode::ContinuousVertical);
    session.viewer_state.viewport_size = Size2::new(200.0, 300.0);

    refresh_continuous_viewer_state(&mut session);
    session.viewer_state.continuous_visible_window = Some(VisiblePageWindow {
        visible_pages: vec![1],
        overdraw_pages: vec![0, 2],
        viewport_top: 950.0,
        viewport_bottom: 1250.0,
    });
    session
        .continuous_scroll
        .record_actual(0, Size2::new(200.0, 100.0));

    refresh_continuous_viewer_state_with_restore(&mut session, false);

    assert_eq!(session.viewer_state.continuous_pending_scroll_top, None);
}

#[test]
fn continuous_dispatch_does_not_insert_textures_or_exceed_cache_capacity() {
    let dir = tempfile::tempdir().expect("dir");
    let archive_path = dir.path().join("book.cbz");
    write_cbz_with_png_pages(&archive_path, 8);
    let mut session = ReadingSession::<&str>::try_new(7, 8, 2).expect("session");

    let submitted = dispatch_continuous_prefetch_for_session(
        &mut session,
        archive_path.to_str().expect("archive path"),
        [0, 1, 2],
    );

    assert_eq!(submitted, 3);
    assert_eq!(session.texture_cache.len(), 0);
    assert_eq!(session.texture_cache.capacity(), 2);
}

#[test]
fn continuous_failed_prefetch_becomes_failed_placeholder() {
    let ctx = egui::Context::default();
    let mut session = ReadingSession::<egui::TextureHandle>::new(7, 6);
    session
        .viewer_state
        .set_layout_mode(ReadingLayoutMode::ContinuousVertical);
    session.prefetch.track_in_flight(
        2,
        DecodeRequestId(77),
        DecodePurpose::Prefetch,
        CancellationToken::new(),
    );

    let inserted = reconcile_prefetch_result(
        &ctx,
        &mut session,
        DecodeResult {
            request_id: DecodeRequestId(77),
            page_index: 2,
            purpose: DecodePurpose::Prefetch,
            outcome: Err(DecodeError::EmptyBytes),
        },
    );
    refresh_continuous_viewer_state(&mut session);

    assert!(!inserted);
    let measurement = session
        .continuous_scroll
        .page_measurements
        .get(&2)
        .expect("failed measurement");
    assert!(measurement.failure.is_some());
    assert!(matches!(
        session.viewer_state.continuous_pages[2].status,
        ContinuousPageStatus::Failed { .. }
    ));
}

fn write_cbz_with_png(path: &std::path::Path) {
    write_cbz_with_png_pages(path, 1);
}

fn write_cbz_with_png_pages(path: &std::path::Path, page_count: usize) {
    let sizes = (0..page_count)
        .map(|page| (12 + page as u32, 18 + page as u32))
        .collect::<Vec<_>>();
    write_cbz_with_png_sizes(path, &sizes);
}

fn write_cbz_with_png_sizes(path: &std::path::Path, sizes: &[(u32, u32)]) {
    let file = std::fs::File::create(path).expect("archive");
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for (page, (width, height)) in sizes.iter().enumerate() {
        archive
            .start_file(format!("{:03}.png", page + 1), options)
            .expect("entry");
        archive
            .write_all(&png_bytes_with_size(*width, *height))
            .expect("png bytes");
    }
    archive.finish().expect("finish");
}

fn png_bytes_with_size(width: u32, height: u32) -> Vec<u8> {
    let image = ImageBuffer::from_pixel(width, height, Rgba([220u8, 40, 80, 255]));
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encode png");
    bytes.into_inner()
}

fn tiny_color_image(size: [usize; 2]) -> egui::ColorImage {
    let [width, height] = size;
    let pixels = vec![egui::Color32::from_rgb(90, 120, 220); width * height];
    egui::ColorImage { size, pixels }
}

fn texture_for(ctx: &egui::Context, name: &str, size: [usize; 2]) -> egui::TextureHandle {
    ctx.load_texture(name, tiny_color_image(size), egui::TextureOptions::LINEAR)
}
