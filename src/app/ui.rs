// ABOUTME: Renders the egui application shell, library controls, and reader toolbar.
// ABOUTME: Connects UI events to the app state without owning archive or storage logic.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, bounded};
use eframe::egui;

use crate::app::{
    AppState, CachedPage, ComicReaderApp, LibraryViewMode, ProgressSnapshot, ReadingSession,
};
use crate::config::{
    AppConfig, default_config_path, default_library_db_path, default_library_store_root,
};
use crate::decode::{
    CancellationToken, DecodePurpose, DecodeRequest, DecodeRequestId, DecodeResult,
    ImageAdjustments, Rotation, decode_page,
};
use crate::library::{
    ActiveLibraryFilter, ComicAvailability, ImportSummary, LibraryGridItem, LibraryGroupKind,
    LibraryService, ThumbnailRequest, ThumbnailStatus, ThumbnailWorkerPool, cache_path_for_source,
    discover_supported_archives, import_paths,
};
use crate::vfs::{ArchiveReader, PdfArchiveReader, RarArchiveReader, ZipArchiveReader};
use crate::viewer::layout::{Size2, ViewMode};
use crate::viewer::{
    self, ContinuousPage, ContinuousPageStatus, PageId, PageNavigationCommand, PageStatus,
    ReadingDirection, ReadingLayoutMode, ViewCommand, ZoomAnchor, anchor_for_viewport,
    build_virtual_canvas, prefetch_candidates, scroll_top_for_anchor,
};

pub const GRID_TILE_WIDTH: f32 = 190.0;
pub const GRID_GAP: f32 = 14.0;
const LIST_THUMBNAIL_WIDTH: f32 = 56.0;
const LIST_THUMBNAIL_HEIGHT: f32 = 74.0;
const EMPTY_LIBRARY_TITLE: &str = "No library loaded";
const EMPTY_LIBRARY_DETAIL: &str = "Add comics from a file or folder to build your library.";
const EDITOR_BACKGROUND: egui::Color32 = egui::Color32::from_rgb(44, 58, 60);
const EDITOR_PANEL: egui::Color32 = egui::Color32::from_rgb(50, 68, 70);
const EDITOR_PANEL_DARK: egui::Color32 = egui::Color32::from_rgb(39, 53, 55);
const EDITOR_PANEL_ACTIVE: egui::Color32 = egui::Color32::from_rgb(63, 98, 104);
const EDITOR_WIDGET: egui::Color32 = egui::Color32::from_rgb(57, 78, 80);
const EDITOR_WIDGET_HOVER: egui::Color32 = egui::Color32::from_rgb(70, 102, 106);
const EDITOR_TEXT: egui::Color32 = egui::Color32::from_rgb(221, 226, 220);
const EDITOR_TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(154, 164, 160);
const EDITOR_GREEN: egui::Color32 = egui::Color32::from_rgb(82, 190, 145);
const EDITOR_CYAN: egui::Color32 = egui::Color32::from_rgb(117, 219, 210);
const EDITOR_ORANGE: egui::Color32 = egui::Color32::from_rgb(222, 129, 70);
const EDITOR_PURPLE: egui::Color32 = egui::Color32::from_rgb(176, 127, 218);

fn editor_panel_frame() -> egui::Frame {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(14))
        .fill(EDITOR_BACKGROUND)
}

fn editor_toolbar_frame() -> egui::Frame {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(12, 8))
        .fill(EDITOR_PANEL_DARK)
}

fn editor_card_stroke() -> egui::Stroke {
    egui::Stroke::new(1.0, EDITOR_GREEN)
}

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
    visible_indices: &[usize],
    selected_ids: &HashSet<i64>,
    thumbnail_textures: &mut HashMap<String, egui::TextureHandle>,
) -> Option<LibraryGridItem> {
    if visible_indices.is_empty() {
        render_empty_library(ui);
        return None;
    }

    let columns = responsive_grid_columns(ui.available_width(), GRID_TILE_WIDTH, GRID_GAP);
    let mut selected = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("library_grid")
            .spacing(egui::vec2(GRID_GAP, GRID_GAP))
            .show(ui, |ui| {
                for (index, item) in visible_indices
                    .iter()
                    .filter_map(|item_index| items.get(*item_index))
                    .enumerate()
                {
                    let is_selected = selected_ids.contains(&item.comic_id);
                    if library_tile(ui, item, is_selected, thumbnail_textures).clicked() {
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
    visible_indices: &[usize],
    selected_ids: &HashSet<i64>,
    thumbnail_textures: &mut HashMap<String, egui::TextureHandle>,
) -> Option<LibraryGridItem> {
    if visible_indices.is_empty() {
        render_empty_library(ui);
        return None;
    }

    let mut selected = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        for item in visible_indices
            .iter()
            .filter_map(|item_index| items.get(*item_index))
        {
            let is_selected = selected_ids.contains(&item.comic_id);
            let response = library_list_row(ui, item, is_selected, thumbnail_textures);
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

fn ensure_bookmarks_loaded(
    app: &mut ComicReaderApp<egui::TextureHandle>,
    library_service: Option<&LibraryService>,
) {
    let Some(service) = library_service else {
        return;
    };
    let Some(session) = &mut app.reading else {
        return;
    };
    if session.bookmarks_loaded {
        return;
    }
    match service.list_bookmarks(session.comic_id) {
        Ok(bookmarks) => {
            session.bookmarks = bookmarks
                .into_iter()
                .map(|bookmark| bookmark.page_index as usize)
                .collect();
        }
        Err(error) => {
            session.viewer_state.chrome.status_text =
                Some(format!("Bookmarks load failed: {error}"));
        }
    }
    session.bookmarks_loaded = true;
}

fn toggle_active_bookmark(
    app: &mut ComicReaderApp<egui::TextureHandle>,
    library_service: Option<&LibraryService>,
) {
    let Some(service) = library_service else {
        return;
    };
    let Some(session) = &mut app.reading else {
        return;
    };
    let page = session.current_page_index;
    match service.toggle_bookmark(session.comic_id, page as u32) {
        Ok(true) => {
            session.bookmarks.insert(page);
        }
        Ok(false) => {
            session.bookmarks.remove(&page);
        }
        Err(error) => {
            session.viewer_state.chrome.status_text = Some(format!("Bookmark failed: {error}"));
        }
    }
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

pub fn resolve_navigation_target(
    command: PageNavigationCommand,
    current_page_index: usize,
    page_count: usize,
) -> usize {
    let last_page = page_count.saturating_sub(1);
    match command {
        PageNavigationCommand::PreviousPage | PageNavigationCommand::ScrollUp => {
            current_page_index.saturating_sub(1)
        }
        PageNavigationCommand::NextPage | PageNavigationCommand::ScrollDown => {
            current_page_index.saturating_add(1).min(last_page)
        }
        PageNavigationCommand::FirstPage => 0,
        PageNavigationCommand::LastPage => last_page,
        PageNavigationCommand::GoToPage(index) => index.min(last_page),
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
    let next_page =
        resolve_navigation_target(command, session.current_page_index, session.page_count);
    let is_jump = matches!(
        command,
        PageNavigationCommand::FirstPage
            | PageNavigationCommand::LastPage
            | PageNavigationCommand::GoToPage(_)
    );

    if Some(next_page)
        != app
            .reading
            .as_ref()
            .map(|session| session.current_page_index)
    {
        load_reader_page(ctx, app, item, next_page);
    }

    // Explicit jumps must move the continuous viewport too, since continuous
    // mode scrolls by viewport offset rather than swapping the current page.
    if is_jump
        && let Some(session) = &mut app.reading
        && session.viewer_state.layout_mode == ReadingLayoutMode::ContinuousVertical
        && let Some(canvas) = &session.viewer_state.continuous_canvas
        && let Some(rect) = canvas
            .page_rects
            .iter()
            .find(|rect| rect.page_index == next_page)
    {
        session.viewer_state.continuous_pending_scroll_top = Some(rect.y);
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
        ViewCommand::RotateLeft => rotate_reader(ctx, app, item, false),
        ViewCommand::RotateRight => rotate_reader(ctx, app, item, true),
        _ => {
            if let Some(session) = &mut app.reading {
                session.viewer_state.pending_view_command = Some(command);
            }
        }
    }
}

fn render_reader_adjustments(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    let mut commit = false;
    {
        let Some(session) = &mut app.reading else {
            return;
        };
        if !session.show_adjustments {
            return;
        }
        let mut open = session.show_adjustments;
        egui::Window::new("Image Adjustments")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut changed = false;
                let mut released = false;
                let mut dragged = false;

                let brightness = ui.add(
                    egui::Slider::new(&mut session.adjustments.brightness, -1.0..=1.0)
                        .text("Brightness"),
                );
                changed |= brightness.changed();
                released |= brightness.drag_stopped();
                dragged |= brightness.dragged();

                let contrast = ui.add(
                    egui::Slider::new(&mut session.adjustments.contrast, -1.0..=1.0)
                        .text("Contrast"),
                );
                changed |= contrast.changed();
                released |= contrast.drag_stopped();
                dragged |= contrast.dragged();

                let gamma = ui.add(
                    egui::Slider::new(&mut session.adjustments.gamma, 0.1..=3.0).text("Gamma"),
                );
                changed |= gamma.changed();
                released |= gamma.drag_stopped();
                dragged |= gamma.dragged();

                let grayscale = ui.add_enabled(
                    !session.adjustments.value_study,
                    egui::Slider::new(&mut session.adjustments.grayscale, 0.0..=1.0)
                        .text("Grayscale"),
                );
                changed |= grayscale.changed();
                released |= grayscale.drag_stopped();
                dragged |= grayscale.dragged();

                let value_study_toggle = ui
                    .checkbox(&mut session.adjustments.value_study, "Value study")
                    .on_hover_text("Posterize luma to N bands (grayscale)");
                if value_study_toggle.changed() {
                    changed = true;
                    released = true;
                }
                let bands = ui.add_enabled(
                    session.adjustments.value_study,
                    egui::Slider::new(
                        &mut session.adjustments.value_bands,
                        crate::decode::VALUE_BANDS_MIN..=crate::decode::VALUE_BANDS_MAX,
                    )
                    .text("Bands"),
                );
                changed |= bands.changed();
                released |= bands.drag_stopped();
                dragged |= bands.dragged();

                if ui.button("Reset").clicked() {
                    session.adjustments = ImageAdjustments::default();
                    changed = true;
                    released = true;
                }

                // Re-decode on release or on a discrete (non-drag) change, so a
                // slider drag does not spam full-page decodes every frame.
                commit = released || (changed && !dragged);
            });
        session.show_adjustments = open;
    }

    if commit {
        let page = if let Some(session) = &mut app.reading {
            session.invalidate_for_adjustments();
            session.current_page_index
        } else {
            return;
        };
        load_reader_page(ctx, app, item, page);
    }
}

fn rotate_reader(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
    rotate_right: bool,
) {
    let page = {
        let Some(session) = &mut app.reading else {
            return;
        };
        let next_rotation = if rotate_right {
            session.rotation.rotate_right()
        } else {
            session.rotation.rotate_left()
        };
        session.set_rotation(next_rotation);
        session.current_page_index
    };
    // Cache was cleared by set_rotation, so this re-decodes the current page in
    // the new orientation; the spread next page and continuous pages refresh on
    // the following frame.
    load_reader_page(ctx, app, item, page);
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

    let already_loading_current = session
        .prefetch
        .in_flight
        .get(&session.current_page_index)
        .is_some_and(|in_flight| {
            in_flight.generation == session.prefetch.generation
                && in_flight.purpose == DecodePurpose::Direct
        });
    if already_loading_current {
        ctx.request_repaint_after(Duration::from_millis(16));
        return;
    }

    if submit_direct_decode_for_session(session, &item.path).is_ok() {
        ctx.request_repaint_after(Duration::from_millis(16));
        return;
    }

    match read_session_page_color_image(session, &item.path, session.current_page_index) {
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
    match read_session_page_color_image(session, &item.path, next_page_index) {
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
        rotation: Rotation::None,
        adjustments: ImageAdjustments::default(),
        cancellation_token: None,
    });
    let color_image = result.outcome.map_err(|err| err.to_string())?;
    let pixel_size = Size2::new(color_image.size[0] as f32, color_image.size[1] as f32);
    Ok((color_image, pixel_size))
}

fn read_session_page_color_image<T>(
    session: &mut ReadingSession<T>,
    archive_path: impl AsRef<Path>,
    page_index: usize,
) -> Result<(egui::ColorImage, Size2), String> {
    let bytes = read_session_page_bytes(session, archive_path.as_ref(), page_index)?;
    let result = decode_page(DecodeRequest {
        request_id: DecodeRequestId(page_index as u64),
        page_index,
        purpose: DecodePurpose::Direct,
        bytes,
        target_size: None,
        rotation: session.rotation,
        adjustments: session.adjustments,
        cancellation_token: None,
    });
    let color_image = result.outcome.map_err(|err| err.to_string())?;
    let pixel_size = Size2::new(color_image.size[0] as f32, color_image.size[1] as f32);
    Ok((color_image, pixel_size))
}

fn submit_direct_decode_for_session<T>(
    session: &mut ReadingSession<T>,
    archive_path: &str,
) -> Result<(), String> {
    let page_index = session.current_page_index;
    let bytes = read_session_page_bytes(session, Path::new(archive_path), page_index)?;
    let worker_pool = session
        .decode_worker_pool
        .as_ref()
        .ok_or_else(|| "Decode worker pool is unavailable".to_owned())?;
    let request_id = session.prefetch.next_request_id();
    let cancellation_token = CancellationToken::new();
    let request = DecodeRequest {
        request_id,
        page_index,
        bytes,
        purpose: DecodePurpose::Direct,
        target_size: None,
        rotation: session.rotation,
        adjustments: session.adjustments,
        cancellation_token: Some(cancellation_token.clone()),
    };
    worker_pool.submit(request).map_err(|err| err.to_string())?;
    session.prefetch.track_in_flight(
        page_index,
        request_id,
        DecodePurpose::Direct,
        cancellation_token,
    );
    Ok(())
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
        let Ok(bytes) = read_session_page_bytes(session, Path::new(archive_path), page_index)
        else {
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
            rotation: session.rotation,
            adjustments: session.adjustments,
            cancellation_token: Some(cancellation_token.clone()),
        };
        if worker_pool.submit(request).is_ok() {
            session.prefetch.track_in_flight(
                page_index,
                request_id,
                DecodePurpose::Prefetch,
                cancellation_token,
            );
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
        let Ok(bytes) = read_session_page_bytes(session, Path::new(archive_path), page_index)
        else {
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
            rotation: session.rotation,
            adjustments: session.adjustments,
            cancellation_token: Some(cancellation_token.clone()),
        };
        if worker_pool.submit(request).is_ok() {
            session.prefetch.track_in_flight(
                page_index,
                request_id,
                DecodePurpose::Prefetch,
                cancellation_token,
            );
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
                format!(
                    "{:?}:{}:{}",
                    result.purpose, result.page_index, result.request_id.0
                ),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            let _ = session.texture_cache.insert(
                result.page_index,
                CachedPage {
                    texture: texture.clone(),
                    pixel_size,
                },
            );
            session
                .continuous_scroll
                .record_actual(result.page_index, pixel_size);
            if result.purpose == DecodePurpose::Direct
                && result.page_index == session.current_page_index
            {
                session.viewer_state.set_ready(
                    PageId(result.page_index as u64),
                    texture,
                    pixel_size,
                );
            }
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
            if result.purpose == DecodePurpose::Direct
                && result.page_index == session.current_page_index
            {
                session
                    .viewer_state
                    .set_failed(PageId(result.page_index as u64), error.to_string());
            }
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

fn read_session_page_bytes<T>(
    session: &mut ReadingSession<T>,
    archive_path: &Path,
    page_index: usize,
) -> Result<Vec<u8>, String> {
    ensure_session_archive_cache(session, archive_path)?;
    session.archive_cache.read_page(page_index)
}

fn ensure_session_archive_cache<T>(
    session: &mut ReadingSession<T>,
    archive_path: &Path,
) -> Result<(), String> {
    if session.archive_cache.is_for(archive_path) {
        return Ok(());
    }
    let mut reader = archive_reader_for_path(archive_path)?;
    let pages = reader.list_pages().map_err(|err| err.to_string())?;
    session.archive_cache.reset(archive_path, reader, pages);
    Ok(())
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
    settings: &mut SettingsWindowState,
    library_service: Option<&LibraryService>,
) {
    poll_decode_results(ctx, app);
    library_controls.poll_import(app, library_service);
    library_controls.poll_thumbnails(ctx, app);
    library_controls.schedule_missing_thumbnails(app);
    if library_controls.is_importing() || library_controls.has_pending_thumbnails() {
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    match app.state {
        AppState::Library => {
            render_library_menu_bar(ctx, app, library_controls, settings, library_service);
            let shelf_selection = render_library_shelf(ctx, app, library_controls);
            if let Some(item) = shelf_selection {
                if app.library.select_mode {
                    app.library.toggle_selection(item.comic_id);
                } else {
                    open_grid_item_in_reader(ctx, app, &item);
                }
            }
            render_about_window(ctx, &mut library_controls.about_open);
            egui::CentralPanel::default()
                .frame(editor_panel_frame())
                .show(ctx, |ui| {
                    render_library_status_and_filter(ui, app);
                    ui.add_space(10.0);
                    let selected = match app.library.view_mode {
                        LibraryViewMode::Thumbnails => render_library_grid::<egui::TextureHandle>(
                            ui,
                            &app.library.items,
                            app.library.visible_indices(),
                            &app.library.selected_ids,
                            &mut library_controls.thumbnail_textures,
                        ),
                        LibraryViewMode::List => render_library_list(
                            ui,
                            &app.library.items,
                            app.library.visible_indices(),
                            &app.library.selected_ids,
                            &mut library_controls.thumbnail_textures,
                        ),
                    };
                    if let Some(item) = selected {
                        if app.library.select_mode {
                            app.library.toggle_selection(item.comic_id);
                        } else {
                            open_grid_item_in_reader(ctx, app, &item);
                        }
                    }
                });
        }
        AppState::Reading(_) => {
            ensure_bookmarks_loaded(app, library_service);
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
            render_reader_menu_bar(ctx, app, library_controls, settings);
            render_reader_nav_bar(ctx, app);
            render_about_window(ctx, &mut library_controls.about_open);
            if let Some(item) = active_library_item(app) {
                process_reader_view_command(ctx, app, &item);
            }
            if let Some(session) = &mut app.reading {
                viewer::ui::render_viewer_panel(ctx, &mut session.viewer_state);
            }
            if app
                .reading
                .as_mut()
                .map(|session| std::mem::take(&mut session.viewer_state.pending_bookmark_toggle))
                .unwrap_or(false)
            {
                toggle_active_bookmark(app, library_service);
            }
            if let Some(item) = active_library_item(app) {
                process_reader_view_command(ctx, app, &item);
                process_reader_navigation(ctx, app, &item);
                render_reader_adjustments(ctx, app, &item);
                dispatch_continuous_if_ready(ctx, app, &item);
                dispatch_prefetch_if_ready(ctx, app, &item);
            }
        }
    }
}

/// Parses a 1-based page number from user input into a clamped 0-based index.
/// Returns None for empty, non-numeric, or zero input.
pub fn parse_goto_target(input: &str, page_count: usize) -> Option<usize> {
    let page_number: usize = input.trim().parse().ok()?;
    if page_number == 0 || page_count == 0 {
        return None;
    }
    Some((page_number - 1).min(page_count - 1))
}

fn render_reader_menu_bar(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    controls: &mut LibraryRootControls,
    settings: &mut SettingsWindowState,
) {
    egui::TopBottomPanel::top("reader_menu_bar")
        .frame(editor_toolbar_frame())
        .resizable(false)
        .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Back to Library").clicked() {
                        ui.close_menu();
                        app.return_to_library();
                    }
                    ui.separator();
                    if ui.button("Settings…").clicked() {
                        ui.close_menu();
                        settings.open = true;
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.close_menu();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Tools", |ui| {
                    let Some(session) = &mut app.reading else {
                        ui.label("No comic open");
                        return;
                    };
                    if ui.button("Rotate left").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_view_command =
                            Some(ViewCommand::RotateLeft);
                    }
                    if ui.button("Rotate right").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_view_command =
                            Some(ViewCommand::RotateRight);
                    }
                    ui.separator();
                    ui.checkbox(&mut session.show_adjustments, "Image adjustments")
                        .on_hover_text("Brightness / contrast / gamma");
                    ui.separator();
                    let current_page = session.current_page_index;
                    let bookmarked = session.bookmarks.contains(&current_page);
                    let bookmark_label = if bookmarked {
                        "★ Remove bookmark"
                    } else {
                        "☆ Add bookmark"
                    };
                    if ui.button(bookmark_label).clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_bookmark_toggle = true;
                    }
                    if !session.bookmarks.is_empty() {
                        ui.menu_button(
                            format!("Bookmarks ({})", session.bookmarks.len()),
                            |ui| {
                                let pages: Vec<usize> =
                                    session.bookmarks.iter().copied().collect();
                                for page in pages {
                                    if ui.button(format!("Page {}", page + 1)).clicked() {
                                        ui.close_menu();
                                        session.viewer_state.pending_navigation =
                                            Some(PageNavigationCommand::GoToPage(page));
                                    }
                                }
                            },
                        );
                    }
                });

                ui.menu_button("Options", |ui| {
                    let Some(session) = &mut app.reading else {
                        ui.label("No comic open");
                        return;
                    };
                    ui.label(egui::RichText::new("Zoom").color(EDITOR_TEXT_MUTED));
                    if ui.button("Fit").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_view_command = Some(ViewCommand::Fit);
                    }
                    if ui.button("Fill").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_view_command = Some(ViewCommand::Fill);
                    }
                    if ui.button("Fit width").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_view_command =
                            Some(ViewCommand::FitWidth);
                    }
                    if ui.button("Fit height").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_view_command =
                            Some(ViewCommand::FitHeight);
                    }
                    if ui.button("Actual size (1:1)").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_view_command =
                            Some(ViewCommand::OneToOne);
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("Layout").color(EDITOR_TEXT_MUTED));
                    let mut continuous_enabled = session.viewer_state.layout_mode
                        == ReadingLayoutMode::ContinuousVertical;
                    if ui.checkbox(&mut continuous_enabled, "Continuous scroll").changed() {
                        session.viewer_state.pending_view_command =
                            Some(ViewCommand::ToggleContinuous);
                    }
                    let mut spread_enabled = session.spread_mode_enabled;
                    let spread_allowed = session.viewer_state.layout_mode
                        != ReadingLayoutMode::ContinuousVertical;
                    if ui
                        .add_enabled_ui(spread_allowed, |ui| {
                            ui.checkbox(&mut spread_enabled, "Two-page spread")
                        })
                        .inner
                        .changed()
                    {
                        session.viewer_state.pending_view_command =
                            Some(ViewCommand::ToggleSpread);
                    }
                    let mut fill_crop = session.viewer_state.view_mode == ViewMode::Fill;
                    if ui.checkbox(&mut fill_crop, "Crop fill").changed() {
                        session.viewer_state.pending_view_command = Some(if fill_crop {
                            ViewCommand::Fill
                        } else {
                            ViewCommand::Fit
                        });
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        ui.close_menu();
                        controls.about_open = true;
                    }
                });
            });
        });
}

fn render_reader_nav_bar(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
) {
    egui::TopBottomPanel::top("reader_nav_bar")
        .frame(editor_toolbar_frame())
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                render_reader_nav_controls(ui, app);
            });
        });
}

fn render_reader_nav_controls(
    ui: &mut egui::Ui,
    app: &mut ComicReaderApp<egui::TextureHandle>,
) {
    let Some(session) = &mut app.reading else {
        return;
    };

    ui.spacing_mut().item_spacing.x = 6.0;
    ui.label(format!(
        "{} / {}",
        session.current_page_index.saturating_add(1),
        session.page_count
    ));
    ui.separator();
    let is_first = session.current_page_index == 0;
    let is_last = session.current_page_index + 1 >= session.page_count;
    if ui
        .add_enabled(!is_first, egui::Button::new("|<").small())
        .on_hover_text("First page (Home)")
        .clicked()
    {
        session.viewer_state.pending_navigation = Some(PageNavigationCommand::FirstPage);
    }
    if ui
        .add_enabled(!is_first, egui::Button::new("Prev").small())
        .clicked()
    {
        session.viewer_state.pending_navigation = Some(PageNavigationCommand::PreviousPage);
    }
    if ui
        .add_enabled(!is_last, egui::Button::new("Next").small())
        .clicked()
    {
        session.viewer_state.pending_navigation = Some(PageNavigationCommand::NextPage);
    }
    if ui
        .add_enabled(!is_last, egui::Button::new(">|").small())
        .on_hover_text("Last page (End)")
        .clicked()
    {
        session.viewer_state.pending_navigation = Some(PageNavigationCommand::LastPage);
    }
    let goto_response = ui.add(
        egui::TextEdit::singleline(&mut session.goto_input)
            .desired_width(46.0)
            .hint_text("page"),
    );
    let go_submitted = ui.button("Go").clicked()
        || (goto_response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
    if go_submitted {
        if let Some(target) = parse_goto_target(&session.goto_input, session.page_count) {
            session.viewer_state.pending_navigation = Some(PageNavigationCommand::GoToPage(target));
        }
        session.goto_input.clear();
    }
    ui.separator();
    let current_page = session.current_page_index;
    let bookmarked = session.bookmarks.contains(&current_page);
    let star = if bookmarked { "★" } else { "☆" };
    if ui
        .add(egui::Button::new(star).small())
        .on_hover_text("Toggle bookmark (B)")
        .clicked()
    {
        session.viewer_state.pending_bookmark_toggle = true;
    }
    ui.separator();
    if ui.button("-").clicked() {
        session.viewer_state.pending_view_command = Some(ViewCommand::ZoomOut);
    }
    let current_zoom = session.viewer_state.zoom_pan.zoom;
    let min_zoom = session.viewer_state.zoom_pan.min_zoom;
    let max_zoom = session.viewer_state.zoom_pan.max_zoom;
    let mut slider_zoom = current_zoom;
    ui.spacing_mut().slider_width = 90.0;
    if ui
        .add(egui::Slider::new(&mut slider_zoom, min_zoom..=max_zoom).show_value(false))
        .changed()
        && current_zoom > 0.0
        && slider_zoom != current_zoom
    {
        session
            .viewer_state
            .zoom_pan
            .apply_zoom_factor(slider_zoom / current_zoom, ZoomAnchor::CENTER);
    }
    ui.label(format!(
        "{:.0}%",
        session.viewer_state.zoom_pan.zoom * 100.0
    ));
    if ui.button("+").clicked() {
        session.viewer_state.pending_view_command = Some(ViewCommand::ZoomIn);
    }
}

pub struct LibraryRootControls {
    import_result_receiver: Option<Receiver<Result<ImportSummary, String>>>,
    store_root: PathBuf,
    thumbnail_pool: Option<ThumbnailWorkerPool>,
    pending_thumbnails: HashSet<String>,
    thumbnail_textures: HashMap<String, egui::TextureHandle>,
    thumbnail_cache_root: PathBuf,
    pub shelf_open: bool,
    pub about_open: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsWindowState {
    pub open: bool,
    pub last_error: Option<String>,
}

impl LibraryRootControls {
    pub fn new() -> Self {
        Self {
            import_result_receiver: None,
            store_root: default_library_store_root(),
            thumbnail_pool: ThumbnailWorkerPool::start(2, 16).ok(),
            pending_thumbnails: HashSet::new(),
            thumbnail_textures: HashMap::new(),
            thumbnail_cache_root: default_thumbnail_cache_root(),
            shelf_open: false,
            about_open: false,
        }
    }

    fn is_importing(&self) -> bool {
        self.import_result_receiver.is_some()
    }

    fn has_pending_thumbnails(&self) -> bool {
        !self.pending_thumbnails.is_empty()
    }

    fn start_import_files(&mut self, files: Vec<PathBuf>) {
        if self.is_importing() || files.is_empty() {
            return;
        }
        let store_root = self.store_root.clone();
        let (sender, receiver) = bounded(1);
        self.import_result_receiver = Some(receiver);
        thread::spawn(move || {
            let _ = sender.send(Ok(import_paths(&files, &store_root)));
        });
    }

    fn start_import_folder(&mut self, folder: PathBuf) {
        if self.is_importing() {
            return;
        }
        let store_root = self.store_root.clone();
        let (sender, receiver) = bounded(1);
        self.import_result_receiver = Some(receiver);
        thread::spawn(move || {
            let result = discover_supported_archives(&folder)
                .map_err(|error| error.to_string())
                .map(|paths| import_paths(&paths, &store_root));
            let _ = sender.send(result);
        });
    }

    fn poll_import(
        &mut self,
        app: &mut ComicReaderApp<egui::TextureHandle>,
        library_service: Option<&LibraryService>,
    ) {
        let Some(receiver) = &self.import_result_receiver else {
            return;
        };
        let Ok(result) = receiver.try_recv() else {
            return;
        };

        self.import_result_receiver = None;
        match result {
            Ok(summary) => {
                let Some(service) = library_service else {
                    app.library.status_text = Some("Library database unavailable".to_owned());
                    return;
                };
                let mut added = 0usize;
                let mut error_text = None;
                for imported in &summary.imported {
                    match service.persist_imported_comic(imported) {
                        Ok(_) => added += 1,
                        Err(error) => error_text = Some(error.to_string()),
                    }
                }
                match service.library_grid_items() {
                    Ok(items) => {
                        app.library.items = items;
                        app.library.refresh_filter_cache();
                    }
                    Err(error) => error_text = Some(error.to_string()),
                }
                app.library.status_text =
                    Some(import_status_text(added, summary.failures.len(), error_text));
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

    fn remove_selected(
        &mut self,
        app: &mut ComicReaderApp<egui::TextureHandle>,
        library_service: Option<&LibraryService>,
    ) {
        let Some(service) = library_service else {
            app.library.status_text = Some("Library database unavailable".to_owned());
            return;
        };
        if app.library.selected_ids.is_empty() {
            return;
        }

        let targets = app
            .library
            .items
            .iter()
            .filter(|item| app.library.selected_ids.contains(&item.comic_id))
            .map(|item| {
                (
                    item.comic_id,
                    item.path.clone(),
                    item.source_fingerprint.clone(),
                )
            })
            .collect::<Vec<_>>();

        let mut removed = 0usize;
        let mut error_text = None;
        for (comic_id, path, fingerprint) in &targets {
            match service.remove_comic(*comic_id) {
                Ok(Some(_)) => {
                    removed += 1;
                    delete_store_file(path, &self.store_root);
                    let thumbnail =
                        cache_path_for_source(&self.thumbnail_cache_root, path, fingerprint);
                    let _ = std::fs::remove_file(&thumbnail);
                    self.thumbnail_textures
                        .remove(&thumbnail.to_string_lossy().into_owned());
                    self.pending_thumbnails.remove(path);
                }
                Ok(None) => {}
                Err(error) => error_text = Some(error.to_string()),
            }
        }

        match service.library_grid_items() {
            Ok(items) => app.library.items = items,
            Err(error) => error_text = Some(error.to_string()),
        }
        app.library.selected_ids.clear();
        app.library.refresh_filter_cache();
        app.library.status_text = Some(error_text.unwrap_or_else(|| {
            format!(
                "Removed {removed} comic{}",
                if removed == 1 { "" } else { "s" }
            )
        }));
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

            let cache_path = cache_path_for_source(
                &self.thumbnail_cache_root,
                &item.path,
                &item.source_fingerprint,
            );
            if cache_path.exists() {
                item.thumbnail_status = ThumbnailStatus::Ready {
                    cache_path: cache_path.to_string_lossy().into_owned(),
                };
                continue;
            }

            match pool.submit(ThumbnailRequest {
                source_path: item.path.clone(),
                source_fingerprint: item.source_fingerprint.clone(),
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

/// Deletes a comic's managed copy and removes its now-empty hash directory.
/// Only files inside the managed store are deleted, so an external original
/// (e.g. a legacy in-place library row) is never touched.
fn delete_store_file(path: &str, store_root: &Path) {
    let path = Path::new(path);
    if !path.starts_with(store_root) {
        return;
    }
    if std::fs::remove_file(path).is_err() {
        return;
    }
    if let Some(parent) = path.parent()
        && parent
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
    {
        let _ = std::fs::remove_dir(parent);
    }
}

fn import_status_text(added: usize, skipped: usize, error: Option<String>) -> String {
    if let Some(error) = error {
        return error;
    }
    let mut text = format!("Added {added} comic{}", if added == 1 { "" } else { "s" });
    if skipped > 0 {
        text.push_str(&format!(" ({skipped} skipped)"));
    }
    text
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
            source_fingerprint: comic.fingerprint.clone(),
            page_count: comic.page_count,
            thumbnail_status: ThumbnailStatus::Missing,
            availability: ComicAvailability::Available,
            subtitle: None,
            series: None,
            series_key: None,
            folder_label: std::path::Path::new(&comic.path)
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|value| value.to_str())
                .map(str::to_owned),
            folder_key: std::path::Path::new(&comic.path)
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned()),
        })
        .collect()
}

pub fn persist_scanned_comics_to_grid_items(
    service: &LibraryService,
    comics: &[crate::library::ScannedComic],
) -> Result<Vec<LibraryGridItem>, String> {
    service
        .reconcile_scanned_comics(comics)
        .map_err(|error| format!("Library update failed: {error}"))?;
    service
        .library_grid_items()
        .map_err(|error| format!("Library load failed: {error}"))
}

fn render_library_menu_bar(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    controls: &mut LibraryRootControls,
    settings: &mut SettingsWindowState,
    library_service: Option<&LibraryService>,
) {
    egui::TopBottomPanel::top("library_menu_bar")
        .frame(editor_toolbar_frame())
        .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    let add_files = ui
                        .add_enabled(
                            !controls.is_importing(),
                            egui::Button::new("Add Files…"),
                        )
                        .clicked();
                    if add_files {
                        ui.close_menu();
                        if let Some(files) = rfd::FileDialog::new()
                            .add_filter("Comics", &["cbz", "cbr", "pdf"])
                            .set_directory(".")
                            .pick_files()
                        {
                            controls.start_import_files(files);
                        }
                    }
                    let add_folder = ui
                        .add_enabled(
                            !controls.is_importing(),
                            egui::Button::new("Add Folder to Library…"),
                        )
                        .clicked();
                    if add_folder {
                        ui.close_menu();
                        if let Some(folder) =
                            rfd::FileDialog::new().set_directory(".").pick_folder()
                        {
                            controls.start_import_folder(folder);
                        }
                    }
                    ui.separator();
                    if ui.button("Settings…").clicked() {
                        ui.close_menu();
                        settings.open = true;
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.close_menu();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Tools", |ui| {
                    ui.label(egui::RichText::new("View").color(EDITOR_TEXT_MUTED));
                    ui.selectable_value(
                        &mut app.library.view_mode,
                        LibraryViewMode::Thumbnails,
                        "Thumbnails",
                    );
                    ui.selectable_value(
                        &mut app.library.view_mode,
                        LibraryViewMode::List,
                        "List",
                    );
                    ui.separator();
                    let mut select_mode = app.library.select_mode;
                    if ui.checkbox(&mut select_mode, "Select mode").changed() {
                        app.library.select_mode = select_mode;
                        if !select_mode {
                            app.library.selected_ids.clear();
                        }
                    }
                    ui.checkbox(&mut controls.shelf_open, "Show shelf");
                    ui.separator();
                    let has_unavailable = app
                        .library
                        .items
                        .iter()
                        .any(|item| item.availability == ComicAvailability::Unavailable);
                    if ui
                        .add_enabled(
                            has_unavailable,
                            egui::Button::new("Purge unavailable"),
                        )
                        .clicked()
                    {
                        ui.close_menu();
                        let purged = app.purge_unavailable_from_view();
                        app.library.status_text =
                            Some(format!("Purged {purged} unavailable comic(s)"));
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        ui.close_menu();
                        controls.about_open = true;
                    }
                });

                if controls.is_importing() {
                    ui.separator();
                    ui.spinner();
                    ui.label(
                        egui::RichText::new("Importing…").color(EDITOR_TEXT_MUTED),
                    );
                }

                if app.library.select_mode {
                    ui.separator();
                    let count = app.library.selected_ids.len();
                    if ui
                        .add_enabled(
                            count > 0,
                            egui::Button::new(format!("Remove selected ({count})")),
                        )
                        .clicked()
                    {
                        controls.remove_selected(app, library_service);
                    }
                }
            });
        });
}

fn render_library_shelf(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    controls: &mut LibraryRootControls,
) -> Option<LibraryGridItem> {
    let mut clicked = None;
    let mut shelf_open = controls.shelf_open;
    egui::SidePanel::left("library_shelf")
        .resizable(true)
        .default_width(220.0)
        .width_range(160.0..=400.0)
        .show_animated(ctx, shelf_open, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Shelf");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").on_hover_text("Hide shelf").clicked() {
                        shelf_open = false;
                    }
                });
            });
            ui.separator();
            let visible: Vec<LibraryGridItem> =
                app.library.visible_items().cloned().collect();
            if visible.is_empty() {
                ui.label(egui::RichText::new("Library is empty").color(EDITOR_TEXT_MUTED));
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for item in &visible {
                    let is_selected = app.library.selected_ids.contains(&item.comic_id)
                        || app.library.selected_comic_id == Some(item.comic_id);
                    let response = ui
                        .selectable_label(is_selected, &item.title)
                        .on_hover_text(&item.path);
                    if response.clicked() {
                        clicked = Some(item.clone());
                    }
                }
            });
        });
    controls.shelf_open = shelf_open;
    clicked
}

fn render_library_status_and_filter(
    ui: &mut egui::Ui,
    app: &mut ComicReaderApp<egui::TextureHandle>,
) {
    render_library_filter_controls(ui, app);
    if let Some(status_text) = &app.library.status_text {
        ui.label(egui::RichText::new(status_text).color(EDITOR_TEXT_MUTED));
    }
}

fn render_about_window(ctx: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    egui::Window::new("About cbr-egui")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("cbr-egui");
            ui.label(
                egui::RichText::new("A CBR/CBZ/PDF comic reader.").color(EDITOR_TEXT_MUTED),
            );
            ui.add_space(6.0);
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
        });
}

fn render_library_filter_controls(
    ui: &mut egui::Ui,
    app: &mut ComicReaderApp<egui::TextureHandle>,
) {
    let groups = app.library.groups().to_vec();
    if groups.is_empty() {
        app.library.active_filter = None;
        return;
    }

    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Filter").color(EDITOR_GREEN));
        let mut next_filter = app.library.active_filter.clone();
        let selected_label = app
            .library
            .active_filter
            .as_ref()
            .and_then(|active| {
                groups
                    .iter()
                    .find(|group| group.kind == active.kind && group.key == active.key)
            })
            .map(group_label)
            .unwrap_or_else(|| "All comics".to_owned());

        egui::ComboBox::from_id_salt("library_filter")
            .selected_text(selected_label)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut next_filter, None, "All comics");
                for group in groups
                    .iter()
                    .filter(|group| group.kind == LibraryGroupKind::Series)
                {
                    ui.selectable_value(
                        &mut next_filter,
                        Some(ActiveLibraryFilter {
                            kind: group.kind,
                            key: group.key.clone(),
                        }),
                        group_label(group),
                    );
                }
                for group in groups
                    .iter()
                    .filter(|group| group.kind == LibraryGroupKind::Folder)
                {
                    ui.selectable_value(
                        &mut next_filter,
                        Some(ActiveLibraryFilter {
                            kind: group.kind,
                            key: group.key.clone(),
                        }),
                        group_label(group),
                    );
                }
            });
        if next_filter != app.library.active_filter {
            app.library.active_filter = next_filter;
            app.library.refresh_filter_cache();
        }
    });
}

fn group_label(group: &crate::library::LibraryGroup) -> String {
    let prefix = match group.kind {
        LibraryGroupKind::Series => "Series",
        LibraryGroupKind::Folder => "Folder",
    };
    format!("{prefix}: {} ({})", group.label, group.item_count)
}

fn ellipsize_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let visible_chars = max_chars.saturating_sub(3);
    let mut shortened = text.chars().take(visible_chars).collect::<String>();
    shortened.push_str("...");
    shortened
}

fn library_tile(
    ui: &mut egui::Ui,
    item: &LibraryGridItem,
    is_selected: bool,
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
                    let fill = if response.hovered() {
                        EDITOR_PANEL_ACTIVE
                    } else {
                        EDITOR_PANEL_DARK
                    };
                    ui.painter().rect_filled(rect, 4.0, fill);
                    ui.painter().rect_stroke(
                        rect,
                        4.0,
                        egui::Stroke::new(1.0, EDITOR_WIDGET_HOVER),
                        egui::StrokeKind::Inside,
                    );
                    egui::Image::new((texture.id(), image_size)).paint_at(ui, image_rect);
                    response
                } else {
                    placeholder_cover_button(ui, thumbnail_size, "Cover failed")
                }
            }
            _ => placeholder_cover_button(ui, thumbnail_size, status),
        };
        if is_selected {
            ui.painter().rect_stroke(
                response.rect,
                4.0,
                egui::Stroke::new(2.0, EDITOR_GREEN),
                egui::StrokeKind::Inside,
            );
        }
        let title_color = if is_selected { EDITOR_GREEN } else { EDITOR_TEXT };
        ui.label(egui::RichText::new(ellipsize_text(&item.title, 42)).color(title_color));
        if let Some(subtitle) = &item.subtitle {
            ui.label(
                egui::RichText::new(ellipsize_text(subtitle, 42))
                    .color(EDITOR_TEXT_MUTED)
                    .small(),
            );
        }
        response
    })
    .inner
}

fn placeholder_cover_button(ui: &mut egui::Ui, size: egui::Vec2, label: &str) -> egui::Response {
    ui.add_sized(
        size,
        egui::Button::new(egui::RichText::new(label).color(EDITOR_TEXT_MUTED))
            .fill(EDITOR_PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, EDITOR_WIDGET_HOVER))
            .wrap(),
    )
}

fn library_list_row(
    ui: &mut egui::Ui,
    item: &LibraryGridItem,
    is_selected: bool,
    thumbnail_textures: &mut HashMap<String, egui::TextureHandle>,
) -> egui::Response {
    let row_height = LIST_THUMBNAIL_HEIGHT + 18.0;
    let available_width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, row_height),
        egui::Sense::click(),
    );

    let row_fill = if response.hovered() {
        EDITOR_PANEL_ACTIVE
    } else {
        EDITOR_PANEL
    };
    ui.painter().rect_filled(rect, 4.0, row_fill);
    let (border_width, border_color) = if is_selected {
        (2.0, EDITOR_GREEN)
    } else {
        (1.0, EDITOR_WIDGET_HOVER)
    };
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(border_width, border_color),
        egui::StrokeKind::Inside,
    );

    let thumb_rect = egui::Rect::from_min_size(
        rect.min + egui::vec2(8.0, 5.0),
        egui::vec2(LIST_THUMBNAIL_WIDTH, LIST_THUMBNAIL_HEIGHT),
    );
    paint_list_thumbnail(ui, item, thumbnail_textures, thumb_rect);

    let text_x = thumb_rect.right() + 12.0;
    let title_pos = egui::pos2(text_x, rect.top() + 12.0);
    let subtitle_pos = egui::pos2(text_x, rect.top() + 33.0);
    let meta_pos = egui::pos2(text_x, rect.top() + 53.0);
    let path_pos = egui::pos2(text_x, rect.top() + 72.0);
    let text_clip = egui::Rect::from_min_max(
        egui::pos2(text_x, rect.top()),
        egui::pos2(rect.right() - 8.0, rect.bottom()),
    );
    let painter = ui.painter().with_clip_rect(text_clip);
    painter.text(
        title_pos,
        egui::Align2::LEFT_TOP,
        ellipsize_text(&item.title, 80),
        egui::FontId::monospace(15.0),
        EDITOR_TEXT,
    );
    if let Some(subtitle) = &item.subtitle {
        painter.text(
            subtitle_pos,
            egui::Align2::LEFT_TOP,
            ellipsize_text(subtitle, 90),
            egui::FontId::monospace(12.0),
            EDITOR_PURPLE,
        );
    }
    painter.text(
        meta_pos,
        egui::Align2::LEFT_TOP,
        format!(
            "{} page{}",
            item.page_count,
            if item.page_count == 1 { "" } else { "s" }
        ),
        egui::FontId::monospace(12.0),
        EDITOR_CYAN,
    );
    painter.text(
        path_pos,
        egui::Align2::LEFT_TOP,
        ellipsize_text(&item.path, 110),
        egui::FontId::monospace(12.0),
        EDITOR_TEXT_MUTED,
    );

    response
}

fn paint_list_thumbnail(
    ui: &mut egui::Ui,
    item: &LibraryGridItem,
    thumbnail_textures: &mut HashMap<String, egui::TextureHandle>,
    rect: egui::Rect,
) {
    ui.painter().rect_filled(rect, 3.0, EDITOR_PANEL_DARK);
    ui.painter().rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, EDITOR_WIDGET_HOVER),
        egui::StrokeKind::Inside,
    );
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
        egui::FontId::monospace(11.0),
        EDITOR_TEXT_MUTED,
    );
}

fn fit_image_size(source: egui::Vec2, bounds: egui::Vec2) -> egui::Vec2 {
    if source.x <= 0.0 || source.y <= 0.0 {
        return bounds;
    }
    let scale = (bounds.x / source.x).min(bounds.y / source.y);
    source * scale
}

fn apply_editor_text_styles(style: &mut egui::Style) {
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::monospace(20.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::monospace(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::monospace(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::monospace(11.0));
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size.y = 26.0;
}

fn editor_dark_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(EDITOR_TEXT);
    visuals.panel_fill = EDITOR_BACKGROUND;
    visuals.window_fill = EDITOR_PANEL;
    visuals.window_stroke = editor_card_stroke();
    visuals.window_corner_radius = 4.0.into();
    visuals.menu_corner_radius = 4.0.into();
    visuals.faint_bg_color = EDITOR_PANEL_DARK;
    visuals.extreme_bg_color = EDITOR_PANEL_DARK;
    visuals.code_bg_color = EDITOR_PANEL_DARK;
    visuals.hyperlink_color = EDITOR_CYAN;
    visuals.warn_fg_color = EDITOR_ORANGE;
    visuals.error_fg_color = egui::Color32::from_rgb(233, 104, 104);
    visuals.selection.bg_fill = EDITOR_PANEL_ACTIVE;
    visuals.selection.stroke = egui::Stroke::new(1.0, EDITOR_CYAN);

    visuals.widgets.noninteractive.bg_fill = EDITOR_PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = EDITOR_PANEL_DARK;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, EDITOR_WIDGET_HOVER);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, EDITOR_TEXT);
    visuals.widgets.noninteractive.corner_radius = 4.0.into();

    visuals.widgets.inactive.bg_fill = EDITOR_WIDGET;
    visuals.widgets.inactive.weak_bg_fill = EDITOR_WIDGET;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, EDITOR_WIDGET_HOVER);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, EDITOR_TEXT);
    visuals.widgets.inactive.corner_radius = 4.0.into();

    visuals.widgets.hovered.bg_fill = EDITOR_WIDGET_HOVER;
    visuals.widgets.hovered.weak_bg_fill = EDITOR_PANEL_ACTIVE;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, EDITOR_CYAN);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, EDITOR_TEXT);
    visuals.widgets.hovered.corner_radius = 4.0.into();

    visuals.widgets.active.bg_fill = EDITOR_PANEL_ACTIVE;
    visuals.widgets.active.weak_bg_fill = EDITOR_PANEL_ACTIVE;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, EDITOR_GREEN);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, EDITOR_CYAN);
    visuals.widgets.active.corner_radius = 4.0.into();

    visuals.widgets.open = visuals.widgets.hovered;
    visuals.button_frame = true;
    visuals.collapsing_header_frame = true;
    visuals.striped = false;
    visuals
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
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub library_service: Option<LibraryService>,
    pub settings: SettingsWindowState,
    last_checkpointed_progress: Option<ProgressSnapshot>,
}

impl EguiComicReaderApp {
    pub fn new() -> Self {
        Self::from_paths(default_config_path(), default_library_db_path())
    }

    pub fn from_paths(config_path: PathBuf, library_db_path: PathBuf) -> Self {
        let config = AppConfig::load(&config_path);
        let library_service = library_db_path
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| {
                LibraryService::initialize(&library_db_path).map_err(std::io::Error::other)
            })
            .ok();

        let mut app = Self {
            inner: ComicReaderApp::default(),
            library_controls: LibraryRootControls::default(),
            config,
            config_path,
            library_service,
            settings: SettingsWindowState::default(),
            last_checkpointed_progress: None,
        };
        app.hydrate_library_from_service();
        if app.config.resume_last_session {
            app.resume_last_session_from_service();
        }
        app.apply_config_to_active_session();
        app
    }

    pub fn with_config_and_service(
        config: AppConfig,
        config_path: PathBuf,
        library_service: LibraryService,
    ) -> Self {
        let mut app = Self {
            inner: ComicReaderApp::default(),
            library_controls: LibraryRootControls::default(),
            config: config.normalized(),
            config_path,
            library_service: Some(library_service),
            settings: SettingsWindowState::default(),
            last_checkpointed_progress: None,
        };
        app.hydrate_library_from_service();
        if app.config.resume_last_session {
            app.resume_last_session_from_service();
        }
        app.apply_config_to_active_session();
        app
    }

    pub fn apply_config_to_context(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        if self.config.dark_mode {
            style.visuals = editor_dark_visuals();
        } else {
            style.visuals = egui::Visuals::light();
        }
        apply_editor_text_styles(&mut style);
        ctx.set_style(style);
    }

    pub fn set_dark_mode(&mut self, dark_mode: bool) {
        self.config.dark_mode = dark_mode;
    }

    pub fn set_zoom_sensitivity(&mut self, zoom_sensitivity: f32) {
        self.config.zoom_sensitivity = crate::config::normalize_zoom_sensitivity(zoom_sensitivity);
        self.apply_config_to_active_session();
    }

    pub fn set_reading_direction(&mut self, reading_direction: ReadingDirection) {
        self.config.reading_direction = reading_direction;
        self.apply_config_to_active_session();
    }

    pub fn hydrate_library_from_service(&mut self) {
        let Some(service) = &self.library_service else {
            return;
        };
        match service.library_grid_items() {
            Ok(items) => {
                self.inner.library.items = items;
                self.inner.library.refresh_filter_cache();
            }
            Err(error) => {
                self.record_lifecycle_error(format!("Library load failed: {error}"));
            }
        }
    }

    pub fn resume_last_session_from_service(&mut self) -> bool {
        let Some(service) = &self.library_service else {
            return false;
        };
        match self.inner.resume_last_session(service) {
            Ok(resumed) => resumed,
            Err(error) => {
                self.record_lifecycle_error(format!("Session resume failed: {error}"));
                false
            }
        }
    }

    pub fn reconcile_resume_state_after_route(&mut self, was_reading: bool) {
        if was_reading && matches!(self.inner.state, AppState::Library) {
            self.config.resume_last_session = false;
            self.last_checkpointed_progress = None;
            if let Err(error) = self.config.save(&self.config_path) {
                self.record_lifecycle_error(format!("Config save failed: {error}"));
            }
        }
    }

    pub fn flush_lifecycle_state(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        let active_snapshot = self.inner.active_progress_snapshot();
        if active_snapshot.is_some() {
            self.config.resume_last_session = true;
        }

        if let Err(error) = self.config.save(&self.config_path) {
            errors.push(format!("Config save failed: {error}"));
        }

        if let (Some(service), Some(snapshot)) = (&self.library_service, active_snapshot)
            && let Err(error) =
                service.save_progress(snapshot.comic_id, snapshot.current_page, snapshot.is_read)
        {
            errors.push(format!("Progress save failed: {error}"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            let message = errors.join("; ");
            self.record_lifecycle_error(message.clone());
            Err(message)
        }
    }

    pub fn checkpoint_active_progress(&mut self) -> Result<(), String> {
        let Some(snapshot) = self.inner.active_progress_snapshot() else {
            return Ok(());
        };
        if self.last_checkpointed_progress == Some(snapshot) {
            return Ok(());
        }
        let Some(service) = &self.library_service else {
            return Ok(());
        };

        match service.save_progress(snapshot.comic_id, snapshot.current_page, snapshot.is_read) {
            Ok(_) => {
                self.config.resume_last_session = true;
                self.last_checkpointed_progress = Some(snapshot);
                Ok(())
            }
            Err(error) => {
                let message = format!("Progress save failed: {error}");
                self.record_lifecycle_error(message.clone());
                Err(message)
            }
        }
    }

    fn apply_config_to_active_session(&mut self) {
        self.inner
            .apply_reading_direction(self.config.reading_direction);
        if let Some(reading) = &mut self.inner.reading {
            reading.viewer_state.zoom_sensitivity = self.config.zoom_sensitivity;
        }
    }

    fn record_lifecycle_error(&mut self, message: String) {
        self.settings.last_error = Some(message.clone());
        self.inner.library.status_text = Some(message.clone());
        if let Some(reading) = &mut self.inner.reading {
            reading.viewer_state.chrome.status_text = Some(message);
        }
    }

    fn render_settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings.open {
            return;
        }

        let mut open = self.settings.open;
        let mut changed = false;
        egui::Window::new("Settings")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                changed |= ui
                    .checkbox(&mut self.config.dark_mode, "Dark mode")
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(
                            &mut self.config.zoom_sensitivity,
                            AppConfig::MIN_ZOOM_SENSITIVITY..=AppConfig::MAX_ZOOM_SENSITIVITY,
                        )
                        .text("Zoom sensitivity"),
                    )
                    .changed();
                ui.horizontal(|ui| {
                    ui.label("Reading direction");
                    changed |= ui
                        .selectable_value(
                            &mut self.config.reading_direction,
                            ReadingDirection::LeftToRight,
                            "LTR",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut self.config.reading_direction,
                            ReadingDirection::RightToLeft,
                            "RTL",
                        )
                        .changed();
                });

                if let Some(error) = &self.settings.last_error {
                    ui.separator();
                    ui.label(egui::RichText::new(error).weak());
                }
            });

        self.settings.open = open;
        if changed {
            self.config = self.config.clone().normalized();
            self.apply_config_to_context(ctx);
            self.apply_config_to_active_session();
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
        self.apply_config_to_context(ctx);
        let was_reading = matches!(self.inner.state, AppState::Reading(_));
        route_app_update(
            ctx,
            &mut self.inner,
            &mut self.library_controls,
            &mut self.settings,
            self.library_service.as_ref(),
        );
        self.reconcile_resume_state_after_route(was_reading);
        let _ = self.checkpoint_active_progress();
        self.render_settings_window(ctx);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        let _ = self.flush_lifecycle_state();
    }
}

impl EguiComicReaderApp {
    pub fn test_instance() -> Self {
        Self {
            inner: ComicReaderApp::default(),
            library_controls: LibraryRootControls::default(),
            config: AppConfig::default(),
            config_path: default_config_path(),
            library_service: None,
            settings: SettingsWindowState::default(),
            last_checkpointed_progress: None,
        }
    }
}
