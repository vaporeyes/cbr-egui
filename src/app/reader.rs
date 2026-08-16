// ABOUTME: Drives the reading session: page loading, prefetch, and navigation.
// ABOUTME: Also renders the reader's own chrome (menu bar, nav bar, sidebar, panels).
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui;
use egui_phosphor::regular as icon;

use crate::app::theme::{
    EDITOR_BACKGROUND, EDITOR_GREEN, EDITOR_PANEL, EDITOR_PANEL_ACTIVE, EDITOR_PANEL_DARK,
    EDITOR_TEXT, EDITOR_TEXT_MUTED, EDITOR_WIDGET_HOVER, editor_toolbar_frame, icon_button,
    icon_button_enabled, icon_text, icon_toggle,
};
use crate::app::ui::{LibraryRootControls, SettingsWindowState, fit_image_size};
use crate::app::{AppState, CachedPage, ComicReaderApp, ReadingSession};
use crate::decode::{
    CancellationToken, DecodePurpose, DecodeRequest, DecodeRequestId, DecodeResult, DecodeSource,
    ImageAdjustments, Rotation, decode_page,
};
use crate::library::{LibraryGridItem, LibraryService};
use crate::vfs::{self, ArchiveReader};
use crate::viewer::layout::{Size2, ViewMode};
use crate::viewer::{
    AppCommand, ContinuousPage, ContinuousPageStatus, PageId, PageNavigationCommand, PageStatus,
    ReadingDirection, ReadingLayoutMode, ViewCommand, ZoomAnchor, anchor_for_viewport,
    build_virtual_canvas, prefetch_candidates, scroll_top_for_anchor,
};

pub(crate) fn active_library_item(
    app: &ComicReaderApp<egui::TextureHandle>,
) -> Option<LibraryGridItem> {
    let AppState::Reading(comic_id) = app.state else {
        return None;
    };
    app.library
        .items
        .iter()
        .find(|item| item.comic_id == comic_id)
        .cloned()
}

pub(crate) fn toggle_active_bookmark(
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

pub(crate) fn ensure_reader_page_loaded(
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
    ensure_spread_next_page_loaded(ctx, app);
}

pub(crate) fn dispatch_prefetch_if_ready(
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

/// Extra texture-cache slots kept beyond the visible+overdraw window so the
/// current page and a little scroll headroom stay resident without thrashing.
const CONTINUOUS_CACHE_MARGIN: usize = 2;

pub(crate) fn dispatch_continuous_if_ready(
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
    // Keep the session's current page in step with the scroll position so the
    // page counter, progress checkpoints, bookmarks, and the sidebar highlight
    // reflect what is actually on screen. The page under the viewport vertical
    // midpoint wins; fall back to the first visible page in gaps.
    let viewport_midpoint = (window.viewport_top + window.viewport_bottom) * 0.5;
    let page_in_view = session
        .viewer_state
        .continuous_canvas
        .as_ref()
        .and_then(|canvas| {
            canvas
                .page_rects
                .iter()
                .find(|rect| rect.y <= viewport_midpoint && rect.bottom() > viewport_midpoint)
                .map(|rect| rect.page_index)
        })
        .or_else(|| window.visible_pages.first().copied());
    if let Some(page_index) = page_in_view {
        session.sync_continuous_current_page(page_index);
    }
    // Size the texture cache to the on-screen working set (+margin for the active
    // page and scroll headroom) so visible pages are never evicted and re-decoded
    // while still in view, which otherwise causes continuous page flicker.
    session
        .texture_cache
        .ensure_capacity(window.all_pages().len() + CONTINUOUS_CACHE_MARGIN);
    if dispatch_continuous_prefetch_for_session(session, &item.path, window.all_pages()) > 0
        || session.prefetch.has_work()
    {
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

/// Maximum decoded pages turned into GPU textures per frame. Bounding the uploads
/// keeps a burst of simultaneously-finished decodes (e.g. four workers completing
/// after a rapid page jump) from blowing the frame budget; the rest stay queued in
/// the channel and are drained on subsequent frames.
const MAX_TEXTURE_UPLOADS_PER_FRAME: usize = 2;

pub(crate) fn poll_decode_results(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
) {
    let Some(session) = &app.reading else {
        return;
    };
    let Some(worker_pool) = &session.decode_worker_pool else {
        return;
    };

    // Pull at most a frame's worth of results; leave any remainder in the channel.
    let mut results = Vec::with_capacity(MAX_TEXTURE_UPLOADS_PER_FRAME);
    while results.len() < MAX_TEXTURE_UPLOADS_PER_FRAME {
        match worker_pool.try_recv() {
            Some(result) => results.push(result),
            None => break,
        }
    }
    let hit_budget = results.len() == MAX_TEXTURE_UPLOADS_PER_FRAME;

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

    if hit_budget {
        // The channel may still hold decoded payloads; come back next frame to
        // continue uploading without stalling this one.
        ctx.request_repaint();
    } else if inserted || session.prefetch.has_work() {
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

pub(crate) fn process_reader_navigation(
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
    // When a two-page spread is on screen, next/previous step over the whole
    // pair so the reader never re-shows a page it just displayed. Landscape
    // singles and pending pairs still step by one.
    let spread_pair_active = session.spread_mode_enabled()
        && session.viewer_state.layout_mode == ReadingLayoutMode::Paged
        && matches!(
            session.viewer_state.spread_decision,
            Some(crate::viewer::SpreadDecision::Pair { .. })
        );
    let last_page = session.page_count.saturating_sub(1);
    let next_page = if spread_pair_active {
        match command {
            PageNavigationCommand::NextPage | PageNavigationCommand::ScrollDown => {
                session.current_page_index.saturating_add(2).min(last_page)
            }
            PageNavigationCommand::PreviousPage | PageNavigationCommand::ScrollUp => {
                session.current_page_index.saturating_sub(2)
            }
            _ => resolve_navigation_target(command, session.current_page_index, session.page_count),
        }
    } else {
        resolve_navigation_target(command, session.current_page_index, session.page_count)
    };
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

/// Carries out the commands the viewer cannot: they need archive access, cache
/// invalidation, or a re-decode. Runs after the viewer has rendered, so a
/// command raised by a keypress during rendering is picked up in the same frame.
pub(crate) fn process_reader_app_command(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    let Some(command) = app
        .reading
        .as_mut()
        .and_then(|session| session.viewer_state.pending_app_command.take())
    else {
        return;
    };

    match command {
        AppCommand::ToggleSpread => toggle_reader_spread(ctx, app),
        AppCommand::ToggleContinuous => toggle_reader_continuous(app),
        AppCommand::RotateLeft => rotate_reader(ctx, app, item, false),
        AppCommand::RotateRight => rotate_reader(ctx, app, item, true),
        AppCommand::ExtractPage => extract_reader_page(app),
    }
}

/// Drops a queued zoom or fit command when the layout has switched to
/// continuous, which has no consumer for them. Without this the command would
/// sit in the slot and fire on returning to the paged renderer.
pub(crate) fn discard_stale_view_command(app: &mut ComicReaderApp<egui::TextureHandle>) {
    if let Some(session) = &mut app.reading
        && session.viewer_state.layout_mode == ReadingLayoutMode::ContinuousVertical
    {
        session.viewer_state.pending_view_command = None;
    }
}

fn extract_reader_page(app: &mut ComicReaderApp<egui::TextureHandle>) {
    let Some(session) = &mut app.reading else {
        return;
    };
    let current_page = session.current_page_index;
    let bytes = match session.archive_cache.read_page(current_page) {
        Ok(bytes) => bytes,
        Err(err) => {
            session.viewer_state.chrome.status_text = Some(format!("Extract failed: {}", err));
            return;
        }
    };

    let file_name = session
        .archive_cache
        .page_entry_path(current_page)
        .and_then(|p| std::path::Path::new(&p).file_name().map(|n| n.to_owned()))
        .and_then(|n| n.to_str().map(|s| s.to_owned()))
        .unwrap_or_else(|| format!("page_{}.jpg", current_page + 1));

    if let Some(target) = rfd::FileDialog::new().set_file_name(&file_name).save_file() {
        if let Err(err) = std::fs::write(&target, bytes) {
            session.viewer_state.chrome.status_text = Some(format!("Extract save failed: {}", err));
        } else {
            session.viewer_state.chrome.status_text =
                Some(format!("Extracted to {}", target.display()));
        }
    }
}

pub(crate) fn render_reader_adjustments(
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

pub(crate) fn render_reader_info_panel(
    ctx: &egui::Context,
    session: &mut ReadingSession<egui::TextureHandle>,
) {
    let mut open = session.show_info_panel;
    egui::Window::new("Comic Info")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            if let Some(metadata) = &session.metadata {
                egui::Grid::new("comic_info_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        if let Some(series) = &metadata.series {
                            ui.label(egui::RichText::new("Series:").strong());
                            ui.label(series);
                            ui.end_row();
                        }
                        if let Some(title) = &metadata.title {
                            ui.label(egui::RichText::new("Title:").strong());
                            ui.label(title);
                            ui.end_row();
                        }
                        if let Some(number) = &metadata.number {
                            ui.label(egui::RichText::new("Number:").strong());
                            ui.label(number);
                            ui.end_row();
                        }
                        if let Some(writer) = &metadata.writer {
                            ui.label(egui::RichText::new("Writer:").strong());
                            ui.label(writer);
                            ui.end_row();
                        }
                        if let Some(penciller) = &metadata.penciller {
                            ui.label(egui::RichText::new("Penciller:").strong());
                            ui.label(penciller);
                            ui.end_row();
                        }
                    });
            } else {
                ui.label(egui::RichText::new("No metadata available for this comic.").weak());
            }
        });
    session.show_info_panel = open;
}

const PAGE_SIDEBAR_THUMB_W: f32 = 110.0;
const PAGE_SIDEBAR_THUMB_H: f32 = 150.0;
const PAGE_SIDEBAR_ROW_HEIGHT: f32 = PAGE_SIDEBAR_THUMB_H + 26.0;
const PAGE_SIDEBAR_THUMB_TARGET: [u32; 2] = [220, 300];
const PAGE_SIDEBAR_MAX_SUBMITS_PER_FRAME: usize = 4;

pub(crate) fn render_reader_page_sidebar(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
) {
    let (show, page_count, current_page, follow_current) = {
        let Some(session) = &mut app.reading else {
            return;
        };
        let current = session.current_page_index;
        // Follow navigation: scroll the sidebar when the current page moved
        // since the last frame, but leave free scrolling alone otherwise.
        let follow = session.sidebar_tracked_page != Some(current);
        session.sidebar_tracked_page = Some(current);
        (
            session.show_page_sidebar,
            session.page_count,
            current,
            follow,
        )
    };
    if !show || page_count == 0 {
        return;
    }

    let mut jump_to: Option<usize> = None;
    let mut visible_pages: Vec<usize> = Vec::new();
    let mut show_flag = show;

    egui::SidePanel::left("reader_page_sidebar")
        .resizable(true)
        .default_width(PAGE_SIDEBAR_THUMB_W + 36.0)
        .width_range(120.0..=260.0)
        .frame(
            egui::Frame::new()
                .inner_margin(egui::Margin::same(8))
                .fill(EDITOR_PANEL_DARK),
        )
        .show_animated(ctx, show_flag, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Pages").color(EDITOR_GREEN));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("✕")
                        .on_hover_text("Hide page sidebar")
                        .clicked()
                    {
                        show_flag = false;
                    }
                });
            });
            ui.separator();

            let mut scroll_area = egui::ScrollArea::vertical().auto_shrink([false; 2]);
            if follow_current {
                let panel_height = ui.available_height();
                let row_step = PAGE_SIDEBAR_ROW_HEIGHT + ui.spacing().item_spacing.y;
                let centered =
                    current_page as f32 * row_step - (panel_height - PAGE_SIDEBAR_ROW_HEIGHT) / 2.0;
                scroll_area = scroll_area.vertical_scroll_offset(centered.max(0.0));
            }
            scroll_area.show_rows(ui, PAGE_SIDEBAR_ROW_HEIGHT, page_count, |ui, row_range| {
                for page_index in row_range.clone() {
                    visible_pages.push(page_index);
                    if render_page_sidebar_row(ui, app, page_index, current_page) {
                        jump_to = Some(page_index);
                    }
                }
            });
        });

    if let Some(session) = &mut app.reading {
        session.show_page_sidebar = show_flag;
    }

    if let Some(page) = jump_to
        && let Some(session) = &mut app.reading
    {
        session.viewer_state.pending_navigation = Some(PageNavigationCommand::GoToPage(page));
    }

    schedule_page_sidebar_thumbnails(app, item, &visible_pages);
}

fn render_page_sidebar_row(
    ui: &mut egui::Ui,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    page_index: usize,
    current_page: usize,
) -> bool {
    let Some(session) = &app.reading else {
        return false;
    };
    let thumbnail = session.page_thumbnails.get(&page_index).cloned();
    let failed = session.failed_page_thumbnails.contains(&page_index);

    let row_size = egui::vec2(ui.available_width(), PAGE_SIDEBAR_ROW_HEIGHT - 4.0);
    let (rect, response) = ui.allocate_exact_size(row_size, egui::Sense::click());

    let is_current = page_index == current_page;
    let fill = if is_current {
        EDITOR_PANEL_ACTIVE
    } else if response.hovered() {
        EDITOR_PANEL
    } else {
        EDITOR_PANEL_DARK
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    if is_current {
        ui.painter().rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(2.0, EDITOR_GREEN),
            egui::StrokeKind::Inside,
        );
    }

    let thumb_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.center().x,
            rect.top() + 6.0 + PAGE_SIDEBAR_THUMB_H / 2.0,
        ),
        egui::vec2(PAGE_SIDEBAR_THUMB_W, PAGE_SIDEBAR_THUMB_H),
    );
    ui.painter().rect_filled(thumb_rect, 3.0, EDITOR_BACKGROUND);
    ui.painter().rect_stroke(
        thumb_rect,
        3.0,
        egui::Stroke::new(1.0, EDITOR_WIDGET_HOVER),
        egui::StrokeKind::Inside,
    );

    if let Some(texture) = &thumbnail {
        let fitted = fit_image_size(texture.size_vec2(), thumb_rect.size());
        let image_rect = egui::Rect::from_center_size(thumb_rect.center(), fitted);
        egui::Image::new((texture.id(), fitted)).paint_at(ui, image_rect);
    } else {
        let label = if failed { "Failed" } else { "Loading" };
        ui.painter().text(
            thumb_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::monospace(11.0),
            EDITOR_TEXT_MUTED,
        );
    }

    let label_color = if is_current {
        EDITOR_GREEN
    } else {
        EDITOR_TEXT
    };
    ui.painter().text(
        egui::pos2(rect.center().x, thumb_rect.bottom() + 6.0),
        egui::Align2::CENTER_TOP,
        format!("{}", page_index + 1),
        egui::FontId::monospace(12.0),
        label_color,
    );

    response.clicked()
}

fn schedule_page_sidebar_thumbnails(
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
    visible_pages: &[usize],
) {
    let Some(session) = &mut app.reading else {
        return;
    };
    if session.page_thumbnail_pool.is_none() {
        return;
    }

    let mut submitted = 0usize;
    for &page_index in visible_pages {
        if submitted >= PAGE_SIDEBAR_MAX_SUBMITS_PER_FRAME {
            break;
        }
        if session.page_thumbnails.contains_key(&page_index)
            || session.pending_page_thumbnails.contains(&page_index)
            || session.failed_page_thumbnails.contains(&page_index)
        {
            continue;
        }

        let page_path = match resolve_session_page_path(session, Path::new(&item.path), page_index)
        {
            Ok(page_path) => page_path,
            Err(_) => {
                session.failed_page_thumbnails.insert(page_index);
                continue;
            }
        };

        let request = DecodeRequest {
            request_id: session.page_thumbnail_request_id(page_index),
            page_index,
            source: DecodeSource::ArchivePage {
                archive_path: PathBuf::from(&item.path),
                page_path,
            },
            purpose: DecodePurpose::Thumbnail,
            target_size: Some(PAGE_SIDEBAR_THUMB_TARGET),
            rotation: session.rotation,
            adjustments: ImageAdjustments::default(),
            cancellation_token: None,
        };

        let Some(pool) = &session.page_thumbnail_pool else {
            break;
        };
        match pool.submit(request) {
            Ok(()) => {
                session.pending_page_thumbnails.insert(page_index);
                submitted += 1;
            }
            Err(_) => break,
        }
    }
}

pub(crate) fn poll_page_thumbnail_results(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
) {
    let Some(session) = &mut app.reading else {
        return;
    };
    let Some(pool) = &session.page_thumbnail_pool else {
        return;
    };

    let mut results = Vec::new();
    while let Some(result) = pool.try_recv() {
        results.push(result);
    }
    if results.is_empty() {
        return;
    }

    for result in results {
        if result.purpose != DecodePurpose::Thumbnail {
            continue;
        }
        // A decode that started before a rotation renders the old orientation.
        // Adopting it would leave the sidebar showing a mix, and because the
        // page then has an entry it would never be requested again.
        if !session.is_current_page_thumbnail(result.request_id) {
            continue;
        }
        session.pending_page_thumbnails.remove(&result.page_index);
        match result.outcome {
            Ok(color_image) => {
                let texture = ctx.load_texture(
                    format!("page_thumb:{}:{}", session.comic_id, result.page_index),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                session.page_thumbnails.insert(result.page_index, texture);
                session.failed_page_thumbnails.remove(&result.page_index);
            }
            Err(_) => {
                session.failed_page_thumbnails.insert(result.page_index);
            }
        }
    }
    ctx.request_repaint();
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
        // current_page_index is kept in sync with the scroll position, so
        // re-applying it routes through set_current_page to refresh paged
        // viewer state (page id, zoom/pan reset) for the page on screen.
        let page_index = session.current_page_index;
        session.set_current_page(page_index);
        session
            .viewer_state
            .set_layout_mode(ReadingLayoutMode::Paged);
    } else {
        // Turn the spread off before switching, while the viewer will still
        // accept the change: set_spread_mode_enabled refuses it once the
        // layout is continuous.
        session.set_spread_mode_enabled(false);
        session
            .viewer_state
            .set_layout_mode(ReadingLayoutMode::ContinuousVertical);
    }
}

fn toggle_reader_spread(ctx: &egui::Context, app: &mut ComicReaderApp<egui::TextureHandle>) {
    let Some(session) = &mut app.reading else {
        return;
    };
    if session.viewer_state.layout_mode == ReadingLayoutMode::ContinuousVertical {
        return;
    }
    let enabled = !session.spread_mode_enabled();
    session.set_spread_mode_enabled(enabled);
    if enabled {
        ensure_spread_next_page_loaded(ctx, app);
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
) {
    let Some(session) = &mut app.reading else {
        return;
    };
    if !session.spread_mode_enabled() {
        return;
    }
    let next_page_index = session.current_page_index.saturating_add(1);
    if next_page_index >= session.page_count {
        session.viewer_state.next_page_status = PageStatus::Empty;
        return;
    }
    let next_page_id = PageId(next_page_index as u64);
    if session.viewer_state.next_page_status.page_id() == Some(next_page_id)
        && session.viewer_state.next_page_status.is_ready()
    {
        return;
    }

    // The prefetcher decodes current+1 off-thread into the texture cache;
    // adopt that texture as soon as it lands instead of blocking the GUI
    // thread on a synchronous decode every page turn.
    if let Some((texture, pixel_size)) = session
        .texture_cache
        .get(next_page_index)
        .map(|cached| (cached.texture.clone(), cached.pixel_size))
    {
        session
            .viewer_state
            .set_next_ready(next_page_id, texture, pixel_size);
        return;
    }

    if let Some(message) = session.prefetch.failed_pages.get(&next_page_index) {
        let message = message.clone();
        session.viewer_state.set_next_failed(next_page_id, message);
        return;
    }

    session.viewer_state.set_next_loading(next_page_id);
    // Come back soon to adopt the prefetch result once the decode completes.
    ctx.request_repaint_after(Duration::from_millis(50));
}

pub fn read_archive_page_color_image(
    archive_path: impl AsRef<Path>,
    page_index: usize,
) -> Result<(egui::ColorImage, Size2), String> {
    let bytes = read_archive_page_bytes(archive_path.as_ref(), page_index)?;
    let result = decode_page(DecodeRequest {
        request_id: DecodeRequestId(page_index as u64),
        page_index,
        source: DecodeSource::Bytes(bytes),
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
        source: DecodeSource::Bytes(bytes),
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
    let page_path = resolve_session_page_path(session, Path::new(archive_path), page_index)?;
    let worker_pool = session
        .decode_worker_pool
        .as_ref()
        .ok_or_else(|| "Decode worker pool is unavailable".to_owned())?;
    let request_id = session.prefetch.next_request_id();
    let cancellation_token = CancellationToken::new();
    let request = DecodeRequest {
        request_id,
        page_index,
        source: DecodeSource::ArchivePage {
            archive_path: PathBuf::from(archive_path),
            page_path,
        },
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
        let Ok(page_path) = resolve_session_page_path(session, Path::new(archive_path), page_index)
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
            source: DecodeSource::ArchivePage {
                archive_path: PathBuf::from(archive_path),
                page_path,
            },
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
        let Ok(page_path) = resolve_session_page_path(session, Path::new(archive_path), page_index)
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
            source: DecodeSource::ArchivePage {
                archive_path: PathBuf::from(archive_path),
                page_path,
            },
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
    // Rebuilding the canvas and page list every frame allocates per page;
    // skip it entirely while the inputs are unchanged.
    let layout_stamp = (
        session.continuous_scroll.measurements_version,
        session.texture_cache.version(),
        viewport_width.to_bits(),
        session.page_count,
    );
    if session.viewer_state.continuous_canvas.is_some()
        && session.continuous_layout_stamp == Some(layout_stamp)
    {
        return;
    }
    session.continuous_layout_stamp = Some(layout_stamp);
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
    } else if session.viewer_state.continuous_canvas.is_none() && session.current_page_index > 0 {
        // First build: jump to the resumed page. Page 0 needs no scroll.
        if let Some(rect) = canvas.page_rects.get(session.current_page_index) {
            session.viewer_state.continuous_pending_scroll_top = Some(rect.y);
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
    vfs::reader_for_path(path).map_err(|err| err.to_string())
}

/// Resolves the archive entry path for a page index without reading its bytes.
/// The cheap page-list lookup stays on the GUI thread; the actual decompression
/// is deferred to the decode worker via `DecodeSource::ArchivePage`.
fn resolve_session_page_path<T>(
    session: &mut ReadingSession<T>,
    archive_path: &Path,
    page_index: usize,
) -> Result<String, String> {
    ensure_session_archive_cache(session, archive_path)?;
    session
        .archive_cache
        .page_entry_path(page_index)
        .ok_or_else(|| format!("Page {} is not available", page_index + 1))
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

pub(crate) fn render_reader_menu_bar(
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
                    if ui.button("Extract current page…").clicked() {
                        ui.close_menu();
                        if let Some(session) = &mut app.reading {
                            session.viewer_state.pending_app_command =
                                Some(AppCommand::ExtractPage);
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
                    let Some(session) = &mut app.reading else {
                        ui.label("No comic open");
                        return;
                    };
                    if ui.button("Rotate left").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_app_command = Some(AppCommand::RotateLeft);
                    }
                    if ui.button("Rotate right").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_app_command = Some(AppCommand::RotateRight);
                    }
                    ui.separator();
                    ui.separator();
                    ui.checkbox(&mut session.show_info_panel, "Comic info")
                        .on_hover_text("View comic metadata (I)");
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
                        ui.menu_button(format!("Bookmarks ({})", session.bookmarks.len()), |ui| {
                            let pages: Vec<usize> = session.bookmarks.iter().copied().collect();
                            for page in pages {
                                if ui.button(format!("Page {}", page + 1)).clicked() {
                                    ui.close_menu();
                                    session.viewer_state.pending_navigation =
                                        Some(PageNavigationCommand::GoToPage(page));
                                }
                            }
                        });
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
                        session.viewer_state.pending_view_command = Some(ViewCommand::FitWidth);
                    }
                    if ui.button("Fit height").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_view_command = Some(ViewCommand::FitHeight);
                    }
                    if ui.button("Actual size (1:1)").clicked() {
                        ui.close_menu();
                        session.viewer_state.pending_view_command = Some(ViewCommand::OneToOne);
                    }
                    ui.separator();
                    ui.checkbox(&mut session.show_page_sidebar, "Page sidebar");
                    ui.separator();
                    ui.label(egui::RichText::new("Layout").color(EDITOR_TEXT_MUTED));
                    let mut continuous_enabled =
                        session.viewer_state.layout_mode == ReadingLayoutMode::ContinuousVertical;
                    if ui
                        .checkbox(&mut continuous_enabled, "Continuous scroll")
                        .changed()
                    {
                        session.viewer_state.pending_app_command =
                            Some(AppCommand::ToggleContinuous);
                    }
                    let mut spread_enabled = session.spread_mode_enabled();
                    let spread_allowed =
                        session.viewer_state.layout_mode != ReadingLayoutMode::ContinuousVertical;
                    if ui
                        .add_enabled_ui(spread_allowed, |ui| {
                            ui.checkbox(&mut spread_enabled, "Two-page spread")
                        })
                        .inner
                        .changed()
                    {
                        session.viewer_state.pending_app_command = Some(AppCommand::ToggleSpread);
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
                    if ui.button("Keyboard Shortcuts").clicked() {
                        ui.close_menu();
                        controls.shortcuts_open = true;
                    }
                    if ui.button("About").clicked() {
                        ui.close_menu();
                        controls.about_open = true;
                    }
                });
            });
        });
}

pub(crate) fn render_reader_nav_bar(
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

fn render_reader_nav_controls(ui: &mut egui::Ui, app: &mut ComicReaderApp<egui::TextureHandle>) {
    if app.reading.is_none() {
        return;
    }

    ui.spacing_mut().item_spacing.x = 4.0;

    if icon_button(ui, icon::ARROW_ARC_LEFT, "Return to library (Esc)").clicked() {
        app.return_to_library();
        return;
    }
    ui.separator();

    let Some(session) = &mut app.reading else {
        return;
    };
    let is_first = session.current_page_index == 0;
    let is_last = session.current_page_index + 1 >= session.page_count;

    // Page navigation. The buttons keep their screen positions, but in RTL
    // the left-pointing arrows advance through the comic, matching how the
    // pages flow.
    let rtl = session.viewer_state.reading_direction == ReadingDirection::RightToLeft;
    let buttons: [(&str, bool, &str, PageNavigationCommand); 4] = if rtl {
        [
            (
                icon::CARET_DOUBLE_LEFT,
                !is_last,
                "Last page (End)",
                PageNavigationCommand::LastPage,
            ),
            (
                icon::CARET_LEFT,
                !is_last,
                "Next page (Left)",
                PageNavigationCommand::NextPage,
            ),
            (
                icon::CARET_RIGHT,
                !is_first,
                "Previous page (Right)",
                PageNavigationCommand::PreviousPage,
            ),
            (
                icon::CARET_DOUBLE_RIGHT,
                !is_first,
                "First page (Home)",
                PageNavigationCommand::FirstPage,
            ),
        ]
    } else {
        [
            (
                icon::CARET_DOUBLE_LEFT,
                !is_first,
                "First page (Home)",
                PageNavigationCommand::FirstPage,
            ),
            (
                icon::CARET_LEFT,
                !is_first,
                "Previous page (Left)",
                PageNavigationCommand::PreviousPage,
            ),
            (
                icon::CARET_RIGHT,
                !is_last,
                "Next page (Right)",
                PageNavigationCommand::NextPage,
            ),
            (
                icon::CARET_DOUBLE_RIGHT,
                !is_last,
                "Last page (End)",
                PageNavigationCommand::LastPage,
            ),
        ]
    };
    for (glyph, enabled, tooltip, command) in buttons {
        if icon_button_enabled(ui, enabled, glyph, tooltip).clicked() {
            session.viewer_state.pending_navigation = Some(command);
        }
    }

    // Page counter and go-to field.
    ui.label(format!(
        "{} / {}",
        session.current_page_index.saturating_add(1),
        session.page_count
    ));
    let goto_response = ui.add(
        egui::TextEdit::singleline(&mut session.goto_input)
            .desired_width(40.0)
            .hint_text("#"),
    );
    let go_submitted =
        goto_response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if go_submitted {
        if let Some(target) = parse_goto_target(&session.goto_input, session.page_count) {
            session.viewer_state.pending_navigation = Some(PageNavigationCommand::GoToPage(target));
        }
        session.goto_input.clear();
    }
    ui.separator();

    // Zoom presets and steppers.
    let view_mode = session.viewer_state.view_mode;
    if icon_toggle(
        ui,
        view_mode == ViewMode::Fit,
        icon::ARROWS_OUT,
        "Fit to window (F)",
    )
    .clicked()
    {
        session.viewer_state.pending_view_command = Some(ViewCommand::Fit);
    }
    if icon_toggle(
        ui,
        view_mode == ViewMode::FitWidth,
        icon::ARROWS_HORIZONTAL,
        "Fit width (W)",
    )
    .clicked()
    {
        session.viewer_state.pending_view_command = Some(ViewCommand::FitWidth);
    }
    if icon_toggle(
        ui,
        view_mode == ViewMode::FitHeight,
        icon::ARROWS_VERTICAL,
        "Fit height (H)",
    )
    .clicked()
    {
        session.viewer_state.pending_view_command = Some(ViewCommand::FitHeight);
    }
    if icon_button(ui, icon::CORNERS_OUT, "Actual size 1:1 (1)").clicked() {
        session.viewer_state.pending_view_command = Some(ViewCommand::OneToOne);
    }
    if icon_button(ui, icon::MAGNIFYING_GLASS_MINUS, "Zoom out (-)").clicked() {
        session.viewer_state.pending_view_command = Some(ViewCommand::ZoomOut);
    }
    let current_zoom = session.viewer_state.zoom_pan.zoom;
    let min_zoom = session.viewer_state.zoom_pan.min_zoom;
    let max_zoom = session.viewer_state.zoom_pan.max_zoom;
    let mut slider_zoom = current_zoom;
    ui.spacing_mut().slider_width = 80.0;
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
    if icon_button(ui, icon::MAGNIFYING_GLASS_PLUS, "Zoom in (+)").clicked() {
        session.viewer_state.pending_view_command = Some(ViewCommand::ZoomIn);
    }
    ui.label(format!(
        "{:.0}%",
        session.viewer_state.zoom_pan.zoom * 100.0
    ));
    ui.separator();

    // Layout toggles.
    let continuous = session.viewer_state.layout_mode == ReadingLayoutMode::ContinuousVertical;
    if icon_toggle(ui, continuous, icon::SCROLL, "Continuous scroll (V)").clicked() {
        session.viewer_state.pending_app_command = Some(AppCommand::ToggleContinuous);
    }
    let spread_allowed = !continuous;
    let spread_active = session.spread_mode_enabled() && spread_allowed;
    if ui
        .add_enabled(
            spread_allowed,
            egui::Button::new(icon_text(icon::BOOK_OPEN)).selected(spread_active),
        )
        .on_hover_text("Two-page spread (S)")
        .clicked()
    {
        session.viewer_state.pending_app_command = Some(AppCommand::ToggleSpread);
    }
    if icon_button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Rotate left (Shift+R)").clicked() {
        session.viewer_state.pending_app_command = Some(AppCommand::RotateLeft);
    }
    if icon_button(ui, icon::ARROW_CLOCKWISE, "Rotate right (R)").clicked() {
        session.viewer_state.pending_app_command = Some(AppCommand::RotateRight);
    }
    ui.separator();

    // Bookmark (highlighted when the current page is bookmarked).
    let bookmarked = session.bookmarks.contains(&session.current_page_index);
    let bookmark_glyph = if bookmarked {
        icon::BOOKMARK
    } else {
        icon::BOOKMARK_SIMPLE
    };
    if icon_toggle(ui, bookmarked, bookmark_glyph, "Toggle bookmark (B)").clicked() {
        session.viewer_state.pending_bookmark_toggle = true;
    }
}
