// ABOUTME: Covers reader lifecycle, import identity, and worker regressions.
// ABOUTME: Exercises public APIs and rendered geometry from the code review.
#[cfg(test)]
mod tests {
    use cbr_egui::library::{
        LibraryService, ThumbnailRequest, ThumbnailWorkerPool, import_comic_file,
    };
    use std::{io::Write, path::Path, sync::mpsc, time::Duration};

    fn cbz(path: &Path, payload: &[u8]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        zip.start_file("page.jpg", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(payload).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn thumbnail_drop_finishes_with_undrained_results() {
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let pool = ThumbnailWorkerPool::start(1, 1).unwrap();
            for _ in 0..2 {
                let request = ThumbnailRequest {
                    source_path: "/missing-review-fixture.cbz".into(),
                    source_fingerprint: "x".into(),
                    cache_path: "/private/tmp/unused-review-cover.png".into(),
                };
                while pool.submit(request.clone()).is_err() {
                    std::thread::yield_now();
                }
            }
            drop(pool);
            done_tx.send(()).unwrap();
        });
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(1)), Ok(()));
    }

    #[test]
    fn reimport_repairs_a_partial_stored_copy() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("book.cbz");
        cbz(&source, b"page");
        let store = dir.path().join("store");
        let imported = import_comic_file(&source, &store).unwrap();
        std::fs::write(&imported.stored_path, b"partial copy").unwrap();
        import_comic_file(&source, &store).unwrap();
        assert_eq!(
            std::fs::read(&imported.stored_path).unwrap(),
            std::fs::read(&source).unwrap()
        );
    }

    #[test]
    fn folder_group_uses_original_source_folder() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("book.cbz");
        cbz(&source, b"page");
        let imported = import_comic_file(&source, &dir.path().join("store")).unwrap();
        let service = LibraryService::initialize(&dir.path().join("library.sqlite")).unwrap();
        service.persist_imported_comic(&imported).unwrap();
        let item = service.library_grid_items().unwrap().remove(0);
        assert_eq!(
            item.folder_label.as_deref(),
            dir.path().file_name().and_then(|name| name.to_str())
        );
    }

    #[test]
    fn renamed_identical_content_reuses_comic() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("book.cbz");
        let renamed = dir.path().join("renamed.cbz");
        cbz(&source, b"page");
        std::fs::copy(&source, &renamed).unwrap();
        let store = dir.path().join("store");
        let service = LibraryService::initialize(&dir.path().join("library.sqlite")).unwrap();
        for path in [&source, &renamed] {
            service
                .persist_imported_comic(&import_comic_file(path, &store).unwrap())
                .unwrap();
        }
        let comics = service.list_comics().unwrap();
        assert_eq!(comics.len(), 1);
    }

    #[test]
    fn failed_pages_are_not_automatically_rescheduled() {
        use cbr_egui::app::{
            ReadingSession,
            reader::{dispatch_continuous_prefetch_for_session, dispatch_prefetch_for_session},
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.cbz");
        cbz(&path, b"invalid image");
        let mut session = ReadingSession::<()>::new(1, 1);
        session
            .prefetch
            .failed_pages
            .insert(0, "invalid image".into());
        assert_eq!(
            dispatch_continuous_prefetch_for_session(&mut session, path.to_str().unwrap(), [0]),
            0
        );
        let mut session = ReadingSession::<()>::new(1, 2);
        session.archive_cache.reset(
            &path,
            Box::new(cbr_egui::vfs::ZipArchiveReader::new(&path)),
            vec![
                cbr_egui::library::ArchivePage {
                    path: "page.jpg".into(),
                    sort_index: 0,
                },
                cbr_egui::library::ArchivePage {
                    path: "page.jpg".into(),
                    sort_index: 1,
                },
            ],
        );
        session
            .prefetch
            .failed_pages
            .insert(1, "invalid image".into());
        assert_eq!(
            dispatch_prefetch_for_session(&mut session, path.to_str().unwrap()),
            0
        );
    }

    #[test]
    fn opening_a_session_applies_reader_preferences() {
        use cbr_egui::{app::ui::EguiComicReaderApp, config::AppConfig, viewer::ReadingDirection};
        let dir = tempfile::tempdir().unwrap();
        let service = LibraryService::initialize(&dir.path().join("library.sqlite")).unwrap();
        let comic = service
            .upsert_comic(cbr_egui::library::ComicInput {
                path: "/book.cbz".into(),
                hash: "x".into(),
                page_count: 10,
                metadata_id: None,
            })
            .unwrap();
        let config = AppConfig {
            reading_direction: ReadingDirection::RightToLeft,
            zoom_sensitivity: 0.008,
            ..AppConfig::default()
        };
        let mut app = EguiComicReaderApp::with_config_and_service(
            config,
            dir.path().join("config.json"),
            service,
        );
        let item = app.inner.library.items[0].clone();
        assert_eq!(comic.id, item.comic_id);
        app.inner
            .open_grid_item_resuming(app.library_service.as_ref().unwrap(), &item);
        let session = app.inner.reading.as_ref().unwrap();
        assert_eq!(
            session.viewer_state.reading_direction,
            ReadingDirection::RightToLeft
        );
        assert_eq!(
            session.viewer_state.zoom_sensitivity,
            app.config.zoom_sensitivity
        );
    }
}

#[cfg(test)]
mod ui_tests {
    use cbr_egui::{
        app::{
            ComicReaderApp,
            ui::{LibraryRootControls, SettingsWindowState, route_app_update},
        },
        config::AppConfig,
        viewer::{
            PageId, PageNavigationCommand, ReadingDirection, ReadingLayoutMode, Size2, ViewerState,
        },
    };
    use eframe::egui;

    #[test]
    fn rtl_spread_preserves_each_pages_aspect_ratio() {
        let ctx = egui::Context::default();
        let first = ctx.load_texture(
            "first",
            egui::ColorImage::new([100, 200], egui::Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let second = ctx.load_texture(
            "second",
            egui::ColorImage::new([150, 200], egui::Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let second_id = second.id();
        let mut viewer = ViewerState::new();
        viewer.set_ready(PageId(0), first, Size2::new(100.0, 200.0));
        viewer.set_next_ready(PageId(1), second, Size2::new(150.0, 200.0));
        viewer.set_spread_mode_enabled(true);
        viewer.set_reading_direction(ReadingDirection::RightToLeft);
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 800.0),
                )),
                ..Default::default()
            },
            |ctx| {
                cbr_egui::viewer::ui::render_viewer_panel(ctx, &mut viewer);
            },
        );
        let rect = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill_texture_id() == second_id => Some(rect.rect),
                _ => None,
            })
            .expect("second image rect");
        assert!((rect.width() / rect.height() - 0.75).abs() < 0.001);
    }

    #[test]
    fn continuous_next_page_requests_a_scroll() {
        let ctx = egui::Context::default();
        let mut app = ComicReaderApp::<egui::TextureHandle>::default();
        let item = cbr_egui::library::LibraryGridItem {
            comic_id: 1,
            title: "Book".into(),
            subtitle: None,
            path: "/missing-review-book.cbz".into(),
            source_fingerprint: "x".into(),
            page_count: 3,
            thumbnail_status: cbr_egui::library::ThumbnailStatus::Failed {
                message: "fixture".into(),
            },
            availability: cbr_egui::library::ComicAvailability::Available,
            series: None,
            series_key: None,
            folder_label: None,
            folder_key: None,
            writer: None,
            number: None,
            is_read: false,
            current_page: 0,
        };
        app.library.items.push(item);
        app.open_comic(1, 3);
        let session = app.reading.as_mut().unwrap();
        session.show_page_sidebar = false;
        session.viewer_state.chrome.visible = false;
        session
            .viewer_state
            .set_layout_mode(ReadingLayoutMode::ContinuousVertical);
        session.viewer_state.set_failed(PageId(0), "fixture");
        session.viewer_state.viewport_size = Size2::new(600.0, 500.0);
        for i in 0..3 {
            session
                .continuous_scroll
                .record_actual(i, Size2::new(600.0, 900.0));
        }
        session.viewer_state.pending_navigation = Some(PageNavigationCommand::NextPage);
        let mut controls = LibraryRootControls::default();
        let mut settings = SettingsWindowState::default();
        let mut config = AppConfig::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 500.0),
                )),
                ..Default::default()
            },
            |ctx| {
                route_app_update(
                    ctx,
                    &mut app,
                    &mut controls,
                    &mut settings,
                    &mut config,
                    None,
                );
            },
        );
        let session = app.reading.as_ref().unwrap();
        assert_eq!(session.current_page_index, 0);
        assert!(
            session
                .viewer_state
                .continuous_pending_scroll_top
                .is_some_and(|top| top > 0.0)
        );
    }
}
