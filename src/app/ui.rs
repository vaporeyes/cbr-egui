use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, bounded};
use eframe::egui;

use crate::app::{AppState, CachedPage, ComicReaderApp, LibraryViewMode, ReadingSession};
use crate::decode::{
    CancellationToken, DecodePurpose, DecodeRequest, DecodeRequestId, DecodeResult, decode_page,
};
use crate::library::{
    ComicAvailability, LibraryGridItem, ThumbnailRequest, ThumbnailStatus, ThumbnailWorkerPool,
    cache_path_for_source, scan_library_root, source_fingerprint,
};
use crate::vfs::{ArchiveReader, PdfArchiveReader, RarArchiveReader, ZipArchiveReader};
use crate::viewer::layout::{Size2, ViewMode};
use crate::viewer::{
    self, ContinuousPage, ContinuousPageStatus, PageId, PageNavigationCommand, PageStatus,
    ReadingLayoutMode, ViewCommand, anchor_for_viewport, build_virtual_canvas, prefetch_candidates,
    scroll_top_for_anchor,
};

pub const GRID_TILE_WIDTH: f32 = 190.0;
pub const GRID_GAP: f32 = 14.0;
const LIST_THUMBNAIL_WIDTH: f32 = 56.0;
const LIST_THUMBNAIL_HEIGHT: f32 = 74.0;
const EMPTY_LIBRARY_TITLE: &str = "No library loaded";
const EMPTY_LIBRARY_DETAIL: &str = "Add a library root to scan comics and build the cover grid.";

pub fn responsive_grid_columns(available_width: f32, tile_width: f32, gap: f32) -> usize {
    if !available_width.is_finite() || available_width <= 0.0 || tile_width <= 0.0 {
        return 1;
    }

    ((available_width + gap) / (tile_width + gap))
        .floor()
        .max(1.0) as usize
}

pub fn render_library_grid<T>(
    ui: &mut egui::Ui,
    items: &[LibraryGridItem],
    thumbnail_textures: &mut HashMap<String, egui::TextureHandle>,
) -> Option<LibraryGridItem> {
    if items.is_empty() {
        render_empty_library(ui);
        return None;
    }

    let columns = responsive_grid_columns(ui.available_width(), GRID_TILE_WIDTH, GRID_GAP);
    let mut selected = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("library_grid")
            .spacing(egui::vec2(GRID_GAP, GRID_GAP))
            .show(ui, |ui| {
                for (index, item) in items.iter().enumerate() {
                    if library_tile(ui, item, thumbnail_textures).clicked() {
                        selected = Some(item.clone());
                    }
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
    });

    selected
}

pub fn render_library_list(
    ui: &mut egui::Ui,
    items: &[LibraryGridItem],
    thumbnail_textures: &mut HashMap<String, egui::TextureHandle>,
) -> Option<LibraryGridItem> {
    if items.is_empty() {
        render_empty_library(ui);
        return None;
    }

    let mut selected = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        for item in items {
            let response = library_list_row(ui, item, thumbnail_textures);
            if response.clicked() {
                selected = Some(item.clone());
            }
        }
    });

    selected
}

pub fn empty_library_text() -> (&'static str, &'static str) {
    (EMPTY_LIBRARY_TITLE, EMPTY_LIBRARY_DETAIL)
}

fn open_grid_item_in_reader(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    if app.open_grid_item(item) {
        load_reader_page(ctx, app, item, 0);
    }
}

fn active_library_item(app: &ComicReaderApp<egui::TextureHandle>) -> Option<LibraryGridItem> {
    let AppState::Reading(comic_id) = app.state else {
        return None;
    };
    app.library
        .items
        .iter()
        .find(|item| item.comic_id == comic_id)
        .cloned()
}

fn ensure_reader_page_loaded(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    let Some(session) = &app.reading else {
        return;
    };
    if session.viewer_state.page_status.is_empty() {
        load_reader_page(ctx, app, item, session.current_page_index);
    }
    ensure_spread_next_page_loaded(ctx, app, item);
}

fn dispatch_prefetch_if_ready(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    let Some(session) = &mut app.reading else {
        return;
    };
    if !session.viewer_state.page_status.is_ready() {
        return;
    }
    if session.viewer_state.layout_mode == ReadingLayoutMode::ContinuousVertical {
        return;
    }
    if dispatch_prefetch_for_session(session, &item.path) > 0 || session.prefetch.has_work() {
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

fn dispatch_continuous_if_ready(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    let Some(session) = &mut app.reading else {
        return;
    };
    if session.viewer_state.layout_mode != ReadingLayoutMode::ContinuousVertical {
        return;
    }
    let Some(window) = session.viewer_state.continuous_visible_window.clone() else {
        return;
    };
    session.continuous_scroll.visible_window = Some(window.clone());
    if dispatch_continuous_prefetch_for_session(session, &item.path, window.all_pages()) > 0
        || session.prefetch.has_work()
    {
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

fn poll_decode_results(ctx: &egui::Context, app: &mut ComicReaderApp<egui::TextureHandle>) {
    let Some(session) = &app.reading else {
        return;
    };
    let Some(worker_pool) = &session.decode_worker_pool else {
        return;
    };

    let mut results = Vec::new();
    while let Some(result) = worker_pool.try_recv() {
        results.push(result);
    }

    if results.is_empty() {
        if session.prefetch.has_work() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        return;
    }

    let Some(session) = &mut app.reading else {
        return;
    };
    let mut inserted = false;
    for result in results {
        inserted |= reconcile_prefetch_result(ctx, session, result);
    }

    if inserted || session.prefetch.has_work() {
        ctx.request_repaint_after(Duration::from_millis(50));
    } else {
        ctx.request_repaint();
    }
}

fn process_reader_navigation(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    let Some(command) = app
        .reading
        .as_mut()
        .and_then(|session| session.viewer_state.pending_navigation.take())
    else {
        return;
    };

    let Some(session) = &app.reading else {
        return;
    };
    let next_page = match command {
        PageNavigationCommand::PreviousPage => session.current_page_index.saturating_sub(1),
        PageNavigationCommand::NextPage | PageNavigationCommand::ScrollDown => session
            .current_page_index
            .saturating_add(1)
            .min(session.page_count.saturating_sub(1)),
        PageNavigationCommand::ScrollUp => session.current_page_index.saturating_sub(1),
    };

    if Some(next_page)
        != app
            .reading
            .as_ref()
            .map(|session| session.current_page_index)
    {
        load_reader_page(ctx, app, item, next_page);
    }
}

fn process_reader_view_command(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    let Some(command) = app
        .reading
        .as_mut()
        .and_then(|session| session.viewer_state.pending_view_command.take())
    else {
        return;
    };

    match command {
        ViewCommand::ToggleSpread => toggle_reader_spread(ctx, app, item),
        ViewCommand::ToggleContinuous => toggle_reader_continuous(app),
        _ => {
            if let Some(session) = &mut app.reading {
                session.viewer_state.pending_view_command = Some(command);
            }
        }
    }
}

fn toggle_reader_continuous(app: &mut ComicReaderApp<egui::TextureHandle>) {
    let Some(session) = &mut app.reading else {
        return;
    };
    if session.viewer_state.layout_mode == ReadingLayoutMode::ContinuousVertical {
        if let Some(window) = &session.viewer_state.continuous_visible_window
            && let Some(page_index) = window.visible_pages.first().copied()
        {
            session.set_current_page(page_index);
        }
        session
            .viewer_state
            .set_layout_mode(ReadingLayoutMode::Paged);
    } else {
        session.set_spread_mode_enabled(false);
        session.spread_mode_enabled = false;
        session
            .viewer_state
            .set_layout_mode(ReadingLayoutMode::ContinuousVertical);
    }
}

fn toggle_reader_spread(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    let Some(session) = &mut app.reading else {
        return;
    };
    if session.viewer_state.layout_mode == ReadingLayoutMode::ContinuousVertical {
        return;
    }
    let enabled = !session.spread_mode_enabled;
    session.set_spread_mode_enabled(enabled);
    if enabled {
        ensure_spread_next_page_loaded(ctx, app, item);
    } else if let Some(session) = &mut app.reading {
        session.viewer_state.next_page_status = PageStatus::Empty;
    }
}

pub fn load_reader_page(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
    page_index: usize,
) {
    let Some(session) = &mut app.reading else {
        return;
    };
    session.set_current_page(page_index);
    let page_id = PageId(session.current_page_index as u64);

    if let Some((texture, pixel_size)) = session
        .texture_cache
        .get(session.current_page_index)
        .map(|cached| (cached.texture.clone(), cached.pixel_size))
    {
        session.viewer_state.set_ready(page_id, texture, pixel_size);
        return;
    }

    session.viewer_state.set_loading(page_id);

    match read_archive_page_color_image(&item.path, session.current_page_index) {
        Ok((color_image, pixel_size)) => {
            let texture = ctx.load_texture(
                format!(
                    "comic:{}:page:{}",
                    item.comic_id, session.current_page_index
                ),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            let _ = session.texture_cache.insert(
                session.current_page_index,
                CachedPage {
                    texture: texture.clone(),
                    pixel_size,
                },
            );
            session
                .continuous_scroll
                .record_actual(session.current_page_index, pixel_size);
            session.viewer_state.set_ready(page_id, texture, pixel_size);
        }
        Err(message) => {
            session.continuous_scroll.record_failure(
                session.current_page_index,
                Size2::ZERO,
                message.clone(),
            );
            session.viewer_state.set_failed(page_id, message);
        }
    }
}

fn ensure_spread_next_page_loaded(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    let Some(session) = &mut app.reading else {
        return;
    };
    if !session.spread_mode_enabled {
        return;
    }
    let next_page_index = session.current_page_index.saturating_add(1);
    if next_page_index >= session.page_count {
        session.viewer_state.next_page_status = PageStatus::Empty;
        return;
    }
    let next_page_id = PageId(next_page_index as u64);
    if session.viewer_state.next_page_status.page_id() == Some(next_page_id)
        && !session.viewer_state.next_page_status.is_failed()
    {
        return;
    }

    session.viewer_state.set_next_loading(next_page_id);
    match read_archive_page_color_image(&item.path, next_page_index) {
        Ok((color_image, pixel_size)) => {
            let texture = ctx.load_texture(
                format!("comic:{}:page:{next_page_index}", item.comic_id),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            session
                .viewer_state
                .set_next_ready(next_page_id, texture, pixel_size);
        }
        Err(message) => {
            session.viewer_state.set_next_failed(next_page_id, message);
        }
    }
}

pub fn read_archive_page_color_image(
    archive_path: impl AsRef<Path>,
    page_index: usize,
) -> Result<(egui::ColorImage, Size2), String> {
    let bytes = read_archive_page_bytes(archive_path.as_ref(), page_index)?;
    let result = decode_page(DecodeRequest {
        request_id: DecodeRequestId(page_index as u64),
        page_index,
        bytes,
        purpose: DecodePurpose::Direct,
        target_size: None,
        cancellation_token: None,
    });
    let color_image = result.outcome.map_err(|err| err.to_string())?;
    let pixel_size = Size2::new(color_image.size[0] as f32, color_image.size[1] as f32);
    Ok((color_image, pixel_size))
}

pub fn dispatch_prefetch_for_session<T>(
    session: &mut ReadingSession<T>,
    archive_path: &str,
) -> usize {
    session.prefetch.cancel_stale(
        session.current_page_index,
        session.page_count,
        session.texture_cache.keys(),
    );
    let candidates = prefetch_candidates(session.current_page_index, &session.prefetch_state());
    let mut submitted = 0;

    for page_index in candidates {
        let Ok(bytes) = read_archive_page_bytes(Path::new(archive_path), page_index) else {
            session
                .prefetch
                .failed_pages
                .insert(page_index, "archive page read failed".to_owned());
            continue;
        };
        let Some(worker_pool) = &session.decode_worker_pool else {
            break;
        };
        let request_id = session.prefetch.next_request_id();
        let cancellation_token = CancellationToken::new();
        let request = DecodeRequest {
            request_id,
            page_index,
            bytes,
            purpose: DecodePurpose::Prefetch,
            target_size: None,
            cancellation_token: Some(cancellation_token.clone()),
        };
        if worker_pool.submit(request).is_ok() {
            session
                .prefetch
                .track_in_flight(page_index, request_id, cancellation_token);
            submitted += 1;
        }
    }

    submitted
}

pub fn dispatch_continuous_prefetch_for_session<T>(
    session: &mut ReadingSession<T>,
    archive_path: &str,
    candidates: impl IntoIterator<Item = usize>,
) -> usize {
    let mut candidates = candidates
        .into_iter()
        .filter(|page_index| *page_index < session.page_count)
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates.dedup();
    session
        .prefetch
        .cancel_stale_except(candidates.iter().copied(), session.texture_cache.keys());

    let mut submitted = 0;
    for page_index in candidates {
        if session.texture_cache.contains(page_index)
            || session.prefetch.queued_pages.contains(&page_index)
            || session.prefetch.in_flight.contains_key(&page_index)
        {
            continue;
        }
        let Ok(bytes) = read_archive_page_bytes(Path::new(archive_path), page_index) else {
            session
                .prefetch
                .failed_pages
                .insert(page_index, "archive page read failed".to_owned());
            session.continuous_scroll.record_failure(
                page_index,
                Size2::ZERO,
                "archive page read failed",
            );
            continue;
        };
        let Some(worker_pool) = &session.decode_worker_pool else {
            break;
        };
        let request_id = session.prefetch.next_request_id();
        let cancellation_token = CancellationToken::new();
        let request = DecodeRequest {
            request_id,
            page_index,
            bytes,
            purpose: DecodePurpose::Prefetch,
            target_size: None,
            cancellation_token: Some(cancellation_token.clone()),
        };
        if worker_pool.submit(request).is_ok() {
            session
                .prefetch
                .track_in_flight(page_index, request_id, cancellation_token);
            submitted += 1;
        }
    }

    submitted
}

pub fn reconcile_prefetch_result(
    ctx: &egui::Context,
    session: &mut ReadingSession<egui::TextureHandle>,
    result: DecodeResult,
) -> bool {
    let Some(_in_flight) = session
        .prefetch
        .complete_fresh_request(result.request_id, result.page_index)
    else {
        return false;
    };

    match result.outcome {
        Ok(color_image) => {
            let pixel_size = Size2::new(color_image.size[0] as f32, color_image.size[1] as f32);
            let texture = ctx.load_texture(
                format!("prefetch:{}:{}", result.page_index, result.request_id.0),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            let _ = session.texture_cache.insert(
                result.page_index,
                CachedPage {
                    texture,
                    pixel_size,
                },
            );
            session
                .continuous_scroll
                .record_actual(result.page_index, pixel_size);
            true
        }
        Err(error) => {
            session
                .prefetch
                .record_failed_page(result.page_index, error.to_string());
            session.continuous_scroll.record_failure(
                result.page_index,
                Size2::ZERO,
                error.to_string(),
            );
            false
        }
    }
}

pub fn refresh_continuous_viewer_state(session: &mut ReadingSession<egui::TextureHandle>) {
    refresh_continuous_viewer_state_with_restore(session, true);
}

pub fn refresh_continuous_viewer_state_with_restore(
    session: &mut ReadingSession<egui::TextureHandle>,
    restore_scroll_anchor: bool,
) {
    if session.viewer_state.layout_mode != ReadingLayoutMode::ContinuousVertical {
        return;
    }
    let viewport_width = session.viewer_state.viewport_size.width.max(1.0);
    let canvas = build_virtual_canvas(
        session.page_count,
        viewport_width,
        &session.continuous_scroll.page_measurements,
        session.continuous_scroll.placeholder_ratio,
        session.continuous_scroll.gap,
    );
    if restore_scroll_anchor
        && let (Some(previous_canvas), Some(window)) = (
            &session.viewer_state.continuous_canvas,
            &session.viewer_state.continuous_visible_window,
        )
        && continuous_canvas_geometry_changed(previous_canvas, &canvas)
    {
        let anchor = anchor_for_viewport(previous_canvas, window.viewport_top);
        let restored_top = scroll_top_for_anchor(&canvas, anchor);
        if (restored_top - window.viewport_top).abs() > 0.5 {
            session.viewer_state.continuous_pending_scroll_top = Some(restored_top);
        }
    }
    let mut pages = Vec::with_capacity(canvas.page_rects.len());
    for rect in &canvas.page_rects {
        let status = if let Some(measurement) = session
            .continuous_scroll
            .page_measurements
            .get(&rect.page_index)
            && let Some(message) = &measurement.failure
        {
            ContinuousPageStatus::Failed {
                message: message.clone(),
            }
        } else if let Some((texture, pixel_size)) = session
            .texture_cache
            .get(rect.page_index)
            .map(|cached| (cached.texture.clone(), cached.pixel_size))
        {
            ContinuousPageStatus::Ready {
                texture,
                pixel_size,
            }
        } else {
            ContinuousPageStatus::Loading
        };
        pages.push(ContinuousPage {
            page_index: rect.page_index,
            rect: *rect,
            status,
        });
    }
    session.viewer_state.continuous_canvas = Some(canvas);
    session.viewer_state.continuous_pages = pages;
}

fn continuous_canvas_geometry_changed(
    previous: &crate::viewer::VirtualCanvas,
    next: &crate::viewer::VirtualCanvas,
) -> bool {
    if (previous.viewport_width - next.viewport_width).abs() > 0.5
        || (previous.total_height - next.total_height).abs() > 0.5
        || previous.page_rects.len() != next.page_rects.len()
    {
        return true;
    }

    previous
        .page_rects
        .iter()
        .zip(&next.page_rects)
        .any(|(previous, next)| {
            previous.page_index != next.page_index
                || (previous.y - next.y).abs() > 0.5
                || (previous.size.width - next.size.width).abs() > 0.5
                || (previous.size.height - next.size.height).abs() > 0.5
        })
}

fn read_archive_page_bytes(archive_path: &Path, page_index: usize) -> Result<Vec<u8>, String> {
    let mut reader = archive_reader_for_path(archive_path)?;
    let pages = reader.list_pages().map_err(|err| err.to_string())?;
    let page = pages
        .get(page_index)
        .ok_or_else(|| format!("Page {} is not available", page_index + 1))?;
    reader.read_page(&page.path).map_err(|err| err.to_string())
}

fn archive_reader_for_path(path: &Path) -> Result<Box<dyn ArchiveReader>, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" | "zip" => Ok(Box::new(ZipArchiveReader::new(path))),
        "cbr" | "rar" => Ok(Box::new(RarArchiveReader::new(path))),
        "pdf" => Ok(Box::new(PdfArchiveReader::new(path))),
        _ => Err(format!("Unsupported archive format: {}", path.display())),
    }
}

fn read_cover_bytes(archive_path: &Path) -> Result<Vec<u8>, String> {
    read_archive_page_bytes(archive_path, 0)
}

fn default_thumbnail_cache_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".cache")
        .join("cbr-egui")
        .join("thumbnails")
}

fn color_image_from_file(path: &str) -> Result<egui::ColorImage, String> {
    let image = image::open(path).map_err(|err| err.to_string())?.to_rgba8();
    let (width, height) = image.dimensions();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        image.as_raw(),
    ))
}

fn load_thumbnail_texture(
    ui: &mut egui::Ui,
    cache_path: &str,
    thumbnail_textures: &mut HashMap<String, egui::TextureHandle>,
) -> Option<egui::TextureHandle> {
    if let Some(texture) = thumbnail_textures.get(cache_path) {
        return Some(texture.clone());
    }

    let color_image = color_image_from_file(cache_path).ok()?;
    let texture = ui.ctx().load_texture(
        format!("thumbnail:{cache_path}"),
        color_image,
        egui::TextureOptions::LINEAR,
    );
    thumbnail_textures.insert(cache_path.to_owned(), texture.clone());
    Some(texture)
}

pub fn route_app_update(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    library_controls: &mut LibraryRootControls,
) {
    poll_decode_results(ctx, app);
    library_controls.poll_scan(app);
    library_controls.poll_thumbnails(ctx, app);
    library_controls.schedule_missing_thumbnails(app);
    if library_controls.is_scanning() || library_controls.has_pending_thumbnails() {
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    match app.state {
        AppState::Library => {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_library_root_controls(ui, app, library_controls);
                ui.separator();
                let selected = match app.library.view_mode {
                    LibraryViewMode::Thumbnails => render_library_grid::<egui::TextureHandle>(
                        ui,
                        &app.library.items,
                        &mut library_controls.thumbnail_textures,
                    ),
                    LibraryViewMode::List => render_library_list(
                        ui,
                        &app.library.items,
                        &mut library_controls.thumbnail_textures,
                    ),
                };
                if let Some(item) = selected {
                    open_grid_item_in_reader(ctx, app, &item);
                }
            });
        }
        AppState::Reading(_) => {
            if let Some(item) = active_library_item(app) {
                ensure_reader_page_loaded(ctx, app, &item);
            }
            let is_scrolling_continuous_view = ctx.input(|input| {
                input.raw_scroll_delta.y != 0.0 || input.smooth_scroll_delta.y != 0.0
            });
            if let Some(session) = &mut app.reading {
                refresh_continuous_viewer_state_with_restore(
                    session,
                    !is_scrolling_continuous_view,
                );
            }
            egui::TopBottomPanel::top("reader_nav")
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Library").clicked() {
                            app.return_to_library();
                        }
                        render_reader_toolbar(ui, app);
                    });
                });
            if let Some(item) = active_library_item(app) {
                process_reader_view_command(ctx, app, &item);
            }
            if let Some(session) = &mut app.reading {
                viewer::ui::render_viewer_panel(ctx, &mut session.viewer_state);
            }
            if let Some(item) = active_library_item(app) {
                process_reader_view_command(ctx, app, &item);
                process_reader_navigation(ctx, app, &item);
                dispatch_continuous_if_ready(ctx, app, &item);
                dispatch_prefetch_if_ready(ctx, app, &item);
            }
        }
    }
}

fn render_reader_toolbar(ui: &mut egui::Ui, app: &mut ComicReaderApp<egui::TextureHandle>) {
    let Some(session) = &mut app.reading else {
        return;
    };

    ui.spacing_mut().item_spacing.x = 6.0;
    ui.separator();
    ui.label(format!(
        "{} / {}",
        session.current_page_index.saturating_add(1),
        session.page_count
    ));
    ui.separator();
    if ui
        .add_enabled(
            session.current_page_index > 0,
            egui::Button::new("Prev").small(),
        )
        .clicked()
    {
        session.viewer_state.pending_navigation = Some(PageNavigationCommand::PreviousPage);
    }
    if ui
        .add_enabled(
            session.current_page_index + 1 < session.page_count,
            egui::Button::new("Next").small(),
        )
        .clicked()
    {
        session.viewer_state.pending_navigation = Some(PageNavigationCommand::NextPage);
    }
    ui.separator();
    if ui.button("Fit").clicked() {
        session.viewer_state.pending_view_command = Some(ViewCommand::Fit);
    }
    if ui.button("Fill").clicked() {
        session.viewer_state.pending_view_command = Some(ViewCommand::Fill);
    }
    if ui.button("1:1").clicked() {
        session.viewer_state.pending_view_command = Some(ViewCommand::OneToOne);
    }
    if ui.button("-").clicked() {
        session.viewer_state.pending_view_command = Some(ViewCommand::ZoomOut);
    }
    ui.label(format!(
        "{:.0}%",
        session.viewer_state.zoom_pan.zoom * 100.0
    ));
    if ui.button("+").clicked() {
        session.viewer_state.pending_view_command = Some(ViewCommand::ZoomIn);
    }
    ui.separator();
    let mut continuous_enabled =
        session.viewer_state.layout_mode == ReadingLayoutMode::ContinuousVertical;
    if ui
        .toggle_value(&mut continuous_enabled, "Continuous")
        .changed()
    {
        session.viewer_state.pending_view_command = Some(ViewCommand::ToggleContinuous);
    }
    let mut spread_enabled = session.spread_mode_enabled;
    if ui
        .add_enabled_ui(
            session.viewer_state.layout_mode != ReadingLayoutMode::ContinuousVertical,
            |ui| ui.toggle_value(&mut spread_enabled, "2 pages"),
        )
        .inner
        .changed()
    {
        session.viewer_state.pending_view_command = Some(ViewCommand::ToggleSpread);
    }
    let mut fill = session.viewer_state.view_mode == ViewMode::Fill;
    if ui.toggle_value(&mut fill, "Crop fill").changed() {
        session.viewer_state.pending_view_command = Some(if fill {
            ViewCommand::Fill
        } else {
            ViewCommand::Fit
        });
    }
}

pub struct LibraryRootControls {
    pub root_path: String,
    scan_result_receiver: Option<Receiver<Result<Vec<LibraryGridItem>, String>>>,
    thumbnail_pool: Option<ThumbnailWorkerPool>,
    pending_thumbnails: HashSet<String>,
    thumbnail_textures: HashMap<String, egui::TextureHandle>,
    thumbnail_cache_root: PathBuf,
}

impl LibraryRootControls {
    pub fn new() -> Self {
        Self {
            root_path: String::new(),
            scan_result_receiver: None,
            thumbnail_pool: ThumbnailWorkerPool::start(2, 16).ok(),
            pending_thumbnails: HashSet::new(),
            thumbnail_textures: HashMap::new(),
            thumbnail_cache_root: default_thumbnail_cache_root(),
        }
    }

    fn is_scanning(&self) -> bool {
        self.scan_result_receiver.is_some()
    }

    fn has_pending_thumbnails(&self) -> bool {
        !self.pending_thumbnails.is_empty()
    }

    fn start_scan(&mut self) {
        if self.root_path.trim().is_empty() || self.is_scanning() {
            return;
        }

        let root_path = self.root_path.trim().to_owned();
        let (sender, receiver) = bounded(1);
        self.scan_result_receiver = Some(receiver);

        thread::spawn(move || {
            let result = scan_library_root(&root_path)
                .map(|comics| scanned_comics_to_grid_items(&comics))
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
    }

    fn poll_scan(&mut self, app: &mut ComicReaderApp<egui::TextureHandle>) {
        let Some(receiver) = &self.scan_result_receiver else {
            return;
        };
        let Ok(result) = receiver.try_recv() else {
            return;
        };

        self.scan_result_receiver = None;
        match result {
            Ok(items) => {
                app.library.items = items;
                app.library.status_text = Some(format!(
                    "Loaded {} comic{}",
                    app.library.items.len(),
                    if app.library.items.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ));
            }
            Err(message) => {
                app.library.status_text = Some(message);
            }
        }
    }

    fn poll_thumbnails(
        &mut self,
        ctx: &egui::Context,
        app: &mut ComicReaderApp<egui::TextureHandle>,
    ) {
        let Some(pool) = &self.thumbnail_pool else {
            return;
        };

        while let Some(result) = pool.try_recv() {
            self.pending_thumbnails.remove(&result.source_path);
            if let Some(item) = app
                .library
                .items
                .iter_mut()
                .find(|item| item.path == result.source_path)
            {
                match result.outcome {
                    Ok(_) => {
                        item.thumbnail_status = ThumbnailStatus::Ready {
                            cache_path: result.cache_path.to_string_lossy().into_owned(),
                        };
                    }
                    Err(message) => {
                        item.thumbnail_status = ThumbnailStatus::Failed { message };
                    }
                }
            }
            ctx.request_repaint();
        }
    }

    fn schedule_missing_thumbnails(&mut self, app: &mut ComicReaderApp<egui::TextureHandle>) {
        let Some(pool) = &self.thumbnail_pool else {
            return;
        };

        let mut scheduled_this_frame = 0;
        for item in &mut app.library.items {
            if scheduled_this_frame >= 2 {
                break;
            }
            if !matches!(
                item.thumbnail_status,
                ThumbnailStatus::Missing | ThumbnailStatus::Stale
            ) || self.pending_thumbnails.contains(&item.path)
            {
                continue;
            }

            let fingerprint = match source_fingerprint(&item.path) {
                Ok(fingerprint) => fingerprint,
                Err(error) => {
                    item.thumbnail_status = ThumbnailStatus::Failed {
                        message: error.to_string(),
                    };
                    continue;
                }
            };
            let cache_path =
                cache_path_for_source(&self.thumbnail_cache_root, &item.path, &fingerprint);
            if cache_path.exists() {
                item.thumbnail_status = ThumbnailStatus::Ready {
                    cache_path: cache_path.to_string_lossy().into_owned(),
                };
                continue;
            }

            let bytes = match read_cover_bytes(Path::new(&item.path)) {
                Ok(bytes) => bytes,
                Err(message) => {
                    item.thumbnail_status = ThumbnailStatus::Failed { message };
                    continue;
                }
            };

            match pool.submit(ThumbnailRequest {
                source_path: item.path.clone(),
                source_fingerprint: fingerprint,
                bytes,
                cache_path,
            }) {
                Ok(()) => {
                    self.pending_thumbnails.insert(item.path.clone());
                    item.thumbnail_status = ThumbnailStatus::Loading;
                    scheduled_this_frame += 1;
                }
                Err(error) => {
                    item.thumbnail_status = ThumbnailStatus::Failed {
                        message: error.to_string(),
                    };
                }
            }
        }
    }
}

impl Default for LibraryRootControls {
    fn default() -> Self {
        Self::new()
    }
}

pub fn scanned_comics_to_grid_items(
    comics: &[crate::library::ScannedComic],
) -> Vec<LibraryGridItem> {
    comics
        .iter()
        .enumerate()
        .map(|(index, comic)| LibraryGridItem {
            comic_id: index as i64 + 1,
            title: std::path::Path::new(&comic.path)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(&comic.path)
                .to_owned(),
            path: comic.path.clone(),
            page_count: comic.page_count,
            thumbnail_status: ThumbnailStatus::Missing,
            availability: ComicAvailability::Available,
        })
        .collect()
}

fn render_library_root_controls(
    ui: &mut egui::Ui,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    controls: &mut LibraryRootControls,
) {
    ui.horizontal(|ui| {
        ui.label("Library root");
        let response = ui.add(
            egui::TextEdit::singleline(&mut controls.root_path)
                .desired_width(420.0)
                .hint_text("/path/to/comics"),
        );
        let enter_pressed =
            response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let open_clicked = ui
            .add_enabled(!controls.is_scanning(), egui::Button::new("Open Folder"))
            .clicked();
        let scan_clicked = ui
            .add_enabled(!controls.is_scanning(), egui::Button::new("Scan"))
            .clicked();
        if open_clicked
            && let Some(folder) = rfd::FileDialog::new().set_directory(".").pick_folder()
        {
            controls.root_path = folder.to_string_lossy().into_owned();
            controls.start_scan();
        }
        if enter_pressed || scan_clicked {
            controls.start_scan();
        }
        if controls.is_scanning() {
            ui.spinner();
        }
        ui.separator();
        ui.selectable_value(
            &mut app.library.view_mode,
            LibraryViewMode::Thumbnails,
            "Thumbnails",
        );
        ui.selectable_value(&mut app.library.view_mode, LibraryViewMode::List, "List");
    });

    if let Some(status_text) = &app.library.status_text {
        ui.label(egui::RichText::new(status_text).weak());
    }

    if app
        .library
        .items
        .iter()
        .any(|item| item.availability == ComicAvailability::Unavailable)
        && ui.button("Purge unavailable").clicked()
    {
        let purged = app.purge_unavailable_from_view();
        app.library.status_text = Some(format!("Purged {purged} unavailable comic(s)"));
    }
}

fn library_tile(
    ui: &mut egui::Ui,
    item: &LibraryGridItem,
    thumbnail_textures: &mut HashMap<String, egui::TextureHandle>,
) -> egui::Response {
    let status = match &item.thumbnail_status {
        ThumbnailStatus::Missing => "No cover",
        ThumbnailStatus::Loading => "Loading cover",
        ThumbnailStatus::Ready { .. } => "Cover ready",
        ThumbnailStatus::Failed { .. } => "Cover failed",
        ThumbnailStatus::Stale => "Cover stale",
    };

    ui.vertical(|ui| {
        ui.set_width(GRID_TILE_WIDTH);
        let thumbnail_size = egui::vec2(GRID_TILE_WIDTH, 240.0);
        let response = match &item.thumbnail_status {
            ThumbnailStatus::Ready { cache_path } => {
                if let Some(texture) = load_thumbnail_texture(ui, cache_path, thumbnail_textures) {
                    let image_size = fit_image_size(texture.size_vec2(), thumbnail_size);
                    let (rect, response) =
                        ui.allocate_exact_size(thumbnail_size, egui::Sense::click());
                    let image_rect = egui::Rect::from_center_size(rect.center(), image_size);
                    ui.painter()
                        .rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
                    egui::Image::new((texture.id(), image_size)).paint_at(ui, image_rect);
                    response
                } else {
                    ui.add_sized(thumbnail_size, egui::Button::new("Cover failed").wrap())
                }
            }
            _ => ui.add_sized(thumbnail_size, egui::Button::new(status).wrap()),
        };
        ui.label(&item.title);
        response
    })
    .inner
}

fn library_list_row(
    ui: &mut egui::Ui,
    item: &LibraryGridItem,
    thumbnail_textures: &mut HashMap<String, egui::TextureHandle>,
) -> egui::Response {
    let row_height = LIST_THUMBNAIL_HEIGHT + 10.0;
    let available_width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, row_height),
        egui::Sense::click(),
    );

    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 3.0, ui.visuals().widgets.hovered.bg_fill);
    }

    let thumb_rect = egui::Rect::from_min_size(
        rect.min + egui::vec2(8.0, 5.0),
        egui::vec2(LIST_THUMBNAIL_WIDTH, LIST_THUMBNAIL_HEIGHT),
    );
    paint_list_thumbnail(ui, item, thumbnail_textures, thumb_rect);

    let text_x = thumb_rect.right() + 12.0;
    let title_pos = egui::pos2(text_x, rect.top() + 12.0);
    let meta_pos = egui::pos2(text_x, rect.top() + 34.0);
    let path_pos = egui::pos2(text_x, rect.top() + 56.0);
    ui.painter().text(
        title_pos,
        egui::Align2::LEFT_TOP,
        &item.title,
        egui::FontId::proportional(15.0),
        ui.visuals().text_color(),
    );
    ui.painter().text(
        meta_pos,
        egui::Align2::LEFT_TOP,
        format!(
            "{} page{}",
            item.page_count,
            if item.page_count == 1 { "" } else { "s" }
        ),
        egui::FontId::proportional(12.0),
        ui.visuals().weak_text_color(),
    );
    ui.painter().text(
        path_pos,
        egui::Align2::LEFT_TOP,
        &item.path,
        egui::FontId::proportional(12.0),
        ui.visuals().weak_text_color(),
    );

    response
}

fn paint_list_thumbnail(
    ui: &mut egui::Ui,
    item: &LibraryGridItem,
    thumbnail_textures: &mut HashMap<String, egui::TextureHandle>,
    rect: egui::Rect,
) {
    ui.painter()
        .rect_filled(rect, 3.0, ui.visuals().faint_bg_color);
    match &item.thumbnail_status {
        ThumbnailStatus::Ready { cache_path } => {
            if let Some(texture) = load_thumbnail_texture(ui, cache_path, thumbnail_textures) {
                let image_size = fit_image_size(texture.size_vec2(), rect.size());
                let image_rect = egui::Rect::from_center_size(rect.center(), image_size);
                egui::Image::new((texture.id(), image_size)).paint_at(ui, image_rect);
                return;
            }
            paint_thumbnail_label(ui, rect, "Failed");
        }
        ThumbnailStatus::Loading => paint_thumbnail_label(ui, rect, "Loading"),
        ThumbnailStatus::Failed { .. } => paint_thumbnail_label(ui, rect, "Failed"),
        ThumbnailStatus::Missing | ThumbnailStatus::Stale => {
            paint_thumbnail_label(ui, rect, "Cover")
        }
    }
}

fn paint_thumbnail_label(ui: &mut egui::Ui, rect: egui::Rect, label: &str) {
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.0),
        ui.visuals().weak_text_color(),
    );
}

fn fit_image_size(source: egui::Vec2, bounds: egui::Vec2) -> egui::Vec2 {
    if source.x <= 0.0 || source.y <= 0.0 {
        return bounds;
    }
    let scale = (bounds.x / source.x).min(bounds.y / source.y);
    source * scale
}

fn render_empty_library(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.heading(EMPTY_LIBRARY_TITLE);
            ui.add_space(8.0);
            ui.label(egui::RichText::new(EMPTY_LIBRARY_DETAIL).weak());
        });
    });
}

pub struct EguiComicReaderApp {
    pub inner: ComicReaderApp<egui::TextureHandle>,
    pub library_controls: LibraryRootControls,
}

impl EguiComicReaderApp {
    pub fn new() -> Self {
        Self {
            inner: ComicReaderApp::default(),
            library_controls: LibraryRootControls::default(),
        }
    }
}

impl Default for EguiComicReaderApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for EguiComicReaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        route_app_update(ctx, &mut self.inner, &mut self.library_controls);
    }
}
