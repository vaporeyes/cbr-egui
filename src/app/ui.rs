// ABOUTME: Renders the egui application shell, library controls, and reader toolbar.
// ABOUTME: Connects UI events to the app state without owning archive or storage logic.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, bounded};
use eframe::egui;
use egui_phosphor::regular as icon;

use crate::app::reader::{
    active_library_item, discard_stale_view_command, dispatch_continuous_if_ready,
    dispatch_prefetch_if_ready, ensure_reader_page_loaded, poll_decode_results,
    poll_page_thumbnail_results, process_reader_app_command, process_reader_navigation,
    render_reader_adjustments, render_reader_info_panel, render_reader_menu_bar,
    render_reader_nav_bar, render_reader_page_sidebar, toggle_active_bookmark,
};

// The reader's public surface stays reachable through `app::ui`, which is
// where callers and tests have always found it.
pub use crate::app::reader::{
    dispatch_continuous_prefetch_for_session, dispatch_prefetch_for_session, load_reader_page,
    parse_goto_target, read_archive_page_color_image, reconcile_prefetch_result,
    refresh_continuous_viewer_state, refresh_continuous_viewer_state_with_restore,
    resolve_navigation_target,
};
use crate::app::theme::{
    EDITOR_CYAN, EDITOR_GREEN, EDITOR_ORANGE, EDITOR_PANEL, EDITOR_PANEL_ACTIVE, EDITOR_PANEL_DARK,
    EDITOR_PURPLE, EDITOR_TEXT, EDITOR_TEXT_MUTED, EDITOR_WIDGET_HOVER, apply_editor_text_styles,
    editor_dark_visuals, editor_panel_frame, editor_toolbar_frame, icon_button_enabled, icon_text,
    icon_toggle,
};
use crate::app::{AppState, ComicReaderApp, LibraryViewMode, ProgressSnapshot};

pub use crate::app::theme::install_icon_fonts;
use crate::config::{
    AppConfig, default_config_path, default_library_db_path, default_library_store_root,
};
use crate::library::{
    ActiveLibraryFilter, ComicAvailability, ImportSummary, LibraryGridItem, LibraryGroupKind,
    LibraryService, ScannedComic, ThumbnailCacheError, ThumbnailRequest, ThumbnailStatus,
    ThumbnailWorkerPool, cache_path_for_source, discover_supported_archives, import_paths,
    scan_library_root,
};
use crate::viewer::{self, ReadingDirection};

pub const GRID_TILE_WIDTH: f32 = 190.0;
/// Fixed tile height (cover + single-line title/subtitle/status rows) so the
/// virtualized grid scrolls without row-height drift.
pub const GRID_TILE_HEIGHT: f32 = 320.0;
pub const GRID_GAP: f32 = 14.0;
const LIST_THUMBNAIL_WIDTH: f32 = 56.0;
const LIST_THUMBNAIL_HEIGHT: f32 = 74.0;
const EMPTY_LIBRARY_TITLE: &str = "No library loaded";
const EMPTY_LIBRARY_DETAIL: &str = "Add comics from a file or folder to build your library.";

pub fn responsive_grid_columns(available_width: f32, tile_width: f32, gap: f32) -> usize {
    if !available_width.is_finite() || available_width <= 0.0 || tile_width <= 0.0 {
        return 1;
    }

    ((available_width + gap) / (tile_width + gap))
        .floor()
        .max(1.0) as usize
}

pub enum LibraryItemEvent {
    /// Boxed so the enum stays small. Every visible tile returns an
    /// `Option<LibraryItemEvent>` each frame, and inlining the grid item made
    /// that a ~300 byte move per tile to carry a payload only a click uses.
    Open(Box<LibraryGridItem>),
    SetRead {
        comic_id: i64,
        is_read: bool,
    },
    Remove {
        comic_id: i64,
    },
    /// The cached cover file could not be read back. Emitted from the render
    /// path so the stale entry can be discarded and regenerated.
    CoverUnreadable {
        comic_id: i64,
    },
}

pub fn render_library_grid<T>(
    ui: &mut egui::Ui,
    items: &[LibraryGridItem],
    visible_indices: &[usize],
    selected_ids: &HashSet<i64>,
    thumbnail_textures: &mut lru::LruCache<String, egui::TextureHandle>,
) -> Option<LibraryItemEvent> {
    if visible_indices.is_empty() {
        render_empty_library(ui);
        return None;
    }

    let columns = responsive_grid_columns(ui.available_width(), GRID_TILE_WIDTH, GRID_GAP).max(1);
    let mut event = None;
    let total_rows = visible_indices.len().div_ceil(columns);

    egui::ScrollArea::vertical().show_rows(
        ui,
        GRID_TILE_HEIGHT + GRID_GAP,
        total_rows,
        |ui, row_range| {
            egui::Grid::new("library_grid")
                .spacing(egui::vec2(GRID_GAP, GRID_GAP))
                .min_col_width(GRID_TILE_WIDTH)
                .show(ui, |ui| {
                    for row_index in row_range {
                        for col in 0..columns {
                            let index = row_index * columns + col;
                            if let Some(&item_index) = visible_indices.get(index) {
                                if let Some(item) = items.get(item_index) {
                                    let is_selected = selected_ids.contains(&item.comic_id);
                                    if let Some(e) =
                                        library_tile(ui, item, is_selected, thumbnail_textures)
                                    {
                                        event = Some(e);
                                    }
                                }
                            } else {
                                ui.label("");
                            }
                        }
                        ui.end_row();
                    }
                });
        },
    );

    event
}

pub fn render_library_list(
    ui: &mut egui::Ui,
    items: &[LibraryGridItem],
    visible_indices: &[usize],
    selected_ids: &HashSet<i64>,
    thumbnail_textures: &mut lru::LruCache<String, egui::TextureHandle>,
) -> Option<LibraryItemEvent> {
    if visible_indices.is_empty() {
        render_empty_library(ui);
        return None;
    }

    let mut event = None;
    let row_height = LIST_THUMBNAIL_HEIGHT + 8.0;

    egui::ScrollArea::vertical().show_rows(
        ui,
        row_height + 6.0,
        visible_indices.len(),
        |ui, row_range| {
            ui.spacing_mut().item_spacing.y = 6.0;
            for index in row_range {
                if let Some(&item_index) = visible_indices.get(index)
                    && let Some(item) = items.get(item_index)
                {
                    let is_selected = selected_ids.contains(&item.comic_id);
                    if let Some(e) = library_list_row(ui, item, is_selected, thumbnail_textures) {
                        event = Some(e);
                    }
                }
            }
        },
    );

    event
}

pub fn empty_library_text() -> (&'static str, &'static str) {
    (EMPTY_LIBRARY_TITLE, EMPTY_LIBRARY_DETAIL)
}

fn open_grid_item_in_reader(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    item: &LibraryGridItem,
    library_service: Option<&LibraryService>,
) {
    let opened = match library_service {
        Some(service) => app.open_grid_item_resuming(service, item),
        None => app.open_grid_item(item),
    };
    if opened {
        let page = app
            .reading
            .as_ref()
            .map(|session| session.current_page_index)
            .unwrap_or(0);
        load_reader_page(ctx, app, item, page);
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
    thumbnail_textures: &mut lru::LruCache<String, egui::TextureHandle>,
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
    thumbnail_textures.push(cache_path.to_owned(), texture.clone());
    Some(texture)
}

const STATUS_MESSAGE_LIFETIME_S: f64 = 6.0;

/// Clears a status message a few seconds after its text last changed. The
/// (text, set-time) pair lives in egui temp memory so the many call sites
/// that assign status text stay untouched.
fn expire_status_text(ctx: &egui::Context, id_salt: &str, status: &mut Option<String>) {
    let Some(text) = status.as_ref() else {
        return;
    };
    let id = egui::Id::new(("status_expiry", id_salt));
    let now = ctx.input(|input| input.time);
    let set_at = ctx.data_mut(|data| match data.get_temp::<(String, f64)>(id) {
        Some((stored, at)) if stored == *text => at,
        _ => {
            data.insert_temp(id, (text.clone(), now));
            now
        }
    });
    let age = now - set_at;
    if age >= STATUS_MESSAGE_LIFETIME_S {
        *status = None;
        ctx.data_mut(|data| data.remove::<(String, f64)>(id));
    } else {
        ctx.request_repaint_after(Duration::from_secs_f64(STATUS_MESSAGE_LIFETIME_S - age));
    }
}

fn handle_library_item_event(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    controls: &mut LibraryRootControls,
    library_service: Option<&LibraryService>,
    event: Option<LibraryItemEvent>,
) {
    match event {
        Some(LibraryItemEvent::Open(item)) => {
            if app.library.select_mode {
                app.library.toggle_selection(item.comic_id);
            } else {
                open_grid_item_in_reader(ctx, app, &item, library_service);
            }
        }
        Some(LibraryItemEvent::SetRead { comic_id, is_read }) => {
            if let Some(service) = library_service
                && let Err(err) = app.set_comic_read(service, comic_id, is_read)
            {
                app.library.status_text = Some(format!("Failed to update read state: {err}"));
            }
        }
        Some(LibraryItemEvent::Remove { comic_id }) => {
            controls.remove_comics(app, library_service, &[comic_id]);
        }
        Some(LibraryItemEvent::CoverUnreadable { comic_id }) => {
            controls.discard_unreadable_cover(app, library_service, comic_id);
        }
        None => {}
    }
}

pub fn route_app_update(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    library_controls: &mut LibraryRootControls,
    settings: &mut SettingsWindowState,
    config: &mut AppConfig,
    library_service: Option<&LibraryService>,
) {
    if !ctx.wants_keyboard_input() && ctx.input(|input| input.key_pressed(egui::Key::F11)) {
        let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
    }
    expire_status_text(ctx, "library", &mut app.library.status_text);
    if let Some(session) = &mut app.reading {
        // Loading text self-clears when the page is ready; only persistent
        // messages (errors, confirmations) expire on the timer.
        if !session.viewer_state.page_status.is_loading() {
            expire_status_text(ctx, "reader", &mut session.viewer_state.chrome.status_text);
        }
    }
    poll_decode_results(ctx, app);
    // Files opened from Finder or the command line wait here while another
    // import runs; the repaint scheduled below retries them next frame.
    if !library_controls.is_importing() {
        let external_opens = crate::mac_open::take_pending();
        if !external_opens.is_empty() {
            library_controls.start_import_and_open(external_opens);
        }
    }
    library_controls.poll_import(ctx, app, library_service);
    library_controls.poll_rescan(app, library_service);
    library_controls.poll_thumbnails(ctx, app, library_service);
    library_controls.schedule_missing_thumbnails(app, library_service);
    if library_controls.is_importing()
        || library_controls.is_rescanning()
        || library_controls.has_pending_thumbnails()
    {
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    match app.state {
        AppState::Library => {
            handle_dropped_imports(ctx, app, library_controls);
            render_library_menu_bar(
                ctx,
                app,
                library_controls,
                settings,
                config,
                library_service,
            );
            render_library_toolbar(ctx, app, library_controls, config);
            render_library_shelf(ctx, app, library_controls);
            render_about_window(ctx, &mut library_controls.about_open);
            render_shortcuts_window(ctx, &mut library_controls.shortcuts_open);
            egui::CentralPanel::default()
                .frame(editor_panel_frame())
                .show(ctx, |ui| {
                    render_library_status(ui, app);
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
                    handle_library_item_event(
                        ctx,
                        app,
                        library_controls,
                        library_service,
                        selected,
                    );
                });
        }
        AppState::Reading(_) => {
            // While a text field (go-to-page) is focused, the keyboard belongs
            // to it; Escape then unfocuses the field instead of acting here.
            let text_input_active = ctx.wants_keyboard_input();
            if !text_input_active && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                // Escape closes the topmost open window first; only with no
                // window open does it leave the reader.
                if !close_topmost_reader_window(app, library_controls, settings) {
                    app.return_to_library();
                    return;
                }
            }
            if !text_input_active
                && ctx.input(|input| input.key_pressed(egui::Key::I))
                && let Some(session) = &mut app.reading
            {
                session.show_info_panel = !session.show_info_panel;
            }
            // Tab toggles an immersive mode that hides the toolbars and
            // sidebar, leaving only the page.
            if !text_input_active
                && ctx.input(|input| input.key_pressed(egui::Key::Tab))
                && let Some(session) = &mut app.reading
            {
                session.viewer_state.chrome.visible = !session.viewer_state.chrome.visible;
            }
            let chrome_visible = app
                .reading
                .as_ref()
                .map(|session| session.viewer_state.chrome.visible)
                .unwrap_or(true);

            // Resolved once per frame. This searches the library and clones the
            // item so the reader helpers can take `&mut app`; doing it at each
            // call site meant several copies of a struct of owned strings every
            // frame.
            let active_item = active_library_item(app);

            if let Some(item) = &active_item {
                ensure_reader_page_loaded(ctx, app, item);
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
            if chrome_visible {
                render_reader_menu_bar(ctx, app, library_controls, settings);
                render_reader_nav_bar(ctx, app);
            }
            render_about_window(ctx, &mut library_controls.about_open);
            render_shortcuts_window(ctx, &mut library_controls.shortcuts_open);
            if let Some(item) = &active_item {
                poll_page_thumbnail_results(ctx, app);
                if chrome_visible {
                    render_reader_page_sidebar(ctx, app, item);
                }
            }
            discard_stale_view_command(app);
            if let Some(session) = &mut app.reading
                && session.show_info_panel
            {
                render_reader_info_panel(ctx, session);
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
            if let Some(item) = &active_item {
                process_reader_app_command(ctx, app, item);
                process_reader_navigation(ctx, app, item);
                render_reader_adjustments(ctx, app, item);
                dispatch_continuous_if_ready(ctx, app, item);
                dispatch_prefetch_if_ready(ctx, app, item);
            }
        }
    }
}

/// Starts an import for files or folders dropped onto the library view, and
/// shows a hint overlay while a drag hovers the window.
fn handle_dropped_imports(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    controls: &mut LibraryRootControls,
) {
    let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
    if hovering {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            "drop_hint".into(),
        ));
        let rect = ctx.screen_rect();
        painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(120));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Drop comics or folders to import",
            egui::FontId::proportional(22.0),
            EDITOR_GREEN,
        );
    }

    let dropped = ctx.input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect::<Vec<_>>()
    });
    if dropped.is_empty() {
        return;
    }
    if controls.is_importing() {
        app.library.status_text = Some("Import already in progress".to_owned());
        return;
    }
    controls.start_import_dropped(dropped);
}

/// Closes the topmost open reader window (settings, adjustments, info,
/// shortcuts, about) and reports whether one was closed.
fn close_topmost_reader_window(
    app: &mut ComicReaderApp<egui::TextureHandle>,
    controls: &mut LibraryRootControls,
    settings: &mut SettingsWindowState,
) -> bool {
    if settings.open {
        settings.open = false;
        return true;
    }
    if controls.shortcuts_open {
        controls.shortcuts_open = false;
        return true;
    }
    if controls.about_open {
        controls.about_open = false;
        return true;
    }
    if let Some(session) = &mut app.reading {
        if session.show_adjustments {
            session.show_adjustments = false;
            return true;
        }
        if session.show_info_panel {
            session.show_info_panel = false;
            return true;
        }
    }
    false
}

/// Upper bound on resident cover textures; covers scroll in and out of view,
/// so an unbounded map would otherwise hold every cover ever shown.
const THUMBNAIL_TEXTURE_CACHE_CAPACITY: usize = 256;

/// Comics inspected per frame when looking for covers already sitting in the
/// cache directory. Each check is one stat, so this bounds the syscalls a frame
/// can spend without making a large library crawl.
const MAX_THUMBNAIL_STATS_PER_FRAME: usize = 64;

pub struct LibraryRootControls {
    import_result_receiver: Option<Receiver<Result<ImportSummary, String>>>,
    rescan_result_receiver: Option<Receiver<Result<Vec<ScannedComic>, String>>>,
    open_first_after_import: bool,
    store_root: PathBuf,
    thumbnail_pool: Option<ThumbnailWorkerPool>,
    pending_thumbnails: HashSet<String>,
    thumbnail_textures: lru::LruCache<String, egui::TextureHandle>,
    thumbnail_cache_root: PathBuf,
    pub shelf_open: bool,
    pub about_open: bool,
    pub shortcuts_open: bool,
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
            rescan_result_receiver: None,
            open_first_after_import: false,
            store_root: default_library_store_root(),
            thumbnail_pool: ThumbnailWorkerPool::start(2, 16).ok(),
            pending_thumbnails: HashSet::new(),
            thumbnail_textures: lru::LruCache::new(
                std::num::NonZeroUsize::new(THUMBNAIL_TEXTURE_CACHE_CAPACITY).expect("non-zero"),
            ),
            thumbnail_cache_root: default_thumbnail_cache_root(),
            shelf_open: false,
            about_open: false,
            shortcuts_open: false,
        }
    }

    fn is_importing(&self) -> bool {
        self.import_result_receiver.is_some()
    }

    fn is_rescanning(&self) -> bool {
        self.rescan_result_receiver.is_some()
    }

    fn has_pending_thumbnails(&self) -> bool {
        !self.pending_thumbnails.is_empty()
    }

    /// Walks the managed store and reconciles it against the database. Comics
    /// whose stored copy has been removed behind the app's back become
    /// unavailable, and page counts and metadata are refreshed from the files
    /// that are still there.
    fn start_rescan(&mut self) {
        if self.is_rescanning() || self.is_importing() {
            return;
        }
        let store_root = self.store_root.clone();
        let (sender, receiver) = bounded(1);
        self.rescan_result_receiver = Some(receiver);
        thread::spawn(move || {
            let result = scan_library_root(&store_root).map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
    }

    fn poll_rescan(
        &mut self,
        app: &mut ComicReaderApp<egui::TextureHandle>,
        library_service: Option<&LibraryService>,
    ) {
        let Some(receiver) = &self.rescan_result_receiver else {
            return;
        };
        let Ok(result) = receiver.try_recv() else {
            return;
        };
        self.rescan_result_receiver = None;

        let scanned = match result {
            Ok(scanned) => scanned,
            Err(message) => {
                app.library.status_text = Some(format!("Rescan failed: {message}"));
                return;
            }
        };
        let Some(service) = library_service else {
            app.library.status_text = Some("Library database unavailable".to_owned());
            return;
        };

        match persist_scanned_comics_to_grid_items(service, &scanned) {
            Ok(items) => {
                app.library.items = items;
                app.library.refresh_filter_cache();
                let unavailable = app
                    .library
                    .items
                    .iter()
                    .filter(|item| item.availability == ComicAvailability::Unavailable)
                    .count();
                app.library.status_text = Some(if unavailable > 0 {
                    format!(
                        "Rescanned {} comic(s); {unavailable} no longer on disk",
                        scanned.len()
                    )
                } else {
                    format!("Rescanned {} comic(s)", scanned.len())
                });
            }
            Err(message) => app.library.status_text = Some(message),
        }
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

    /// Imports files requested from outside the UI (Finder open events, CLI
    /// arguments) and opens the first one in the reader once persisted.
    fn start_import_and_open(&mut self, files: Vec<PathBuf>) {
        if self.is_importing() || files.is_empty() {
            return;
        }
        self.open_first_after_import = true;
        self.start_import_files(files);
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

    /// Imports a mixed batch of dropped paths: folders are expanded to their
    /// supported archives on the worker thread, files are imported directly.
    fn start_import_dropped(&mut self, paths: Vec<PathBuf>) {
        if self.is_importing() || paths.is_empty() {
            return;
        }
        let store_root = self.store_root.clone();
        let (sender, receiver) = bounded(1);
        self.import_result_receiver = Some(receiver);
        thread::spawn(move || {
            let mut files = Vec::new();
            let mut discover_error = None;
            for path in paths {
                if path.is_dir() {
                    match discover_supported_archives(&path) {
                        Ok(found) => files.extend(found),
                        Err(error) => discover_error = Some(error.to_string()),
                    }
                } else {
                    files.push(path);
                }
            }
            let result = match discover_error {
                Some(message) if files.is_empty() => Err(message),
                _ => Ok(import_paths(&files, &store_root)),
            };
            let _ = sender.send(result);
        });
    }

    fn poll_import(
        &mut self,
        ctx: &egui::Context,
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
        let open_after_import = std::mem::take(&mut self.open_first_after_import);
        match result {
            Ok(summary) => {
                let Some(service) = library_service else {
                    app.library.status_text = Some("Library database unavailable".to_owned());
                    return;
                };
                let mut added = 0usize;
                let mut already_present = 0usize;
                let mut error_text = None;
                for imported in &summary.imported {
                    match service.persist_imported_comic(imported) {
                        Ok(_) => {
                            if imported.already_present {
                                already_present += 1;
                            } else {
                                added += 1;
                            }
                        }
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
                app.library.status_text = Some(import_status_text(
                    added,
                    already_present,
                    summary.failures.len(),
                    error_text,
                ));
                if open_after_import {
                    let target = summary.imported.first().and_then(|imported| {
                        let stored = imported.stored_path.to_string_lossy();
                        app.library
                            .items
                            .iter()
                            .find(|item| item.path == stored)
                            .cloned()
                    });
                    if let Some(item) = target {
                        open_grid_item_in_reader(ctx, app, &item, library_service);
                    }
                }
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
        library_service: Option<&LibraryService>,
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
                        let cache_path = result.cache_path.to_string_lossy().into_owned();
                        // Record the cover in the database so the next launch
                        // loads it straight from the row instead of rediscovering
                        // every cover through the per-frame scheduler.
                        if let Some(service) = library_service
                            && let Err(error) =
                                service.set_thumbnail_key(&item.path, Some(&cache_path))
                        {
                            app.library.status_text =
                                Some(format!("Cover bookkeeping failed: {error}"));
                        }
                        item.thumbnail_status = ThumbnailStatus::Ready { cache_path };
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
        let comic_ids = app.library.selected_ids.iter().copied().collect::<Vec<_>>();
        self.remove_comics(app, library_service, &comic_ids);
        app.library.selected_ids.clear();
    }

    /// Removes comics from the library along with their managed copies and
    /// cached thumbnails. Used by both multi-select removal and the per-item
    /// context menu.
    fn remove_comics(
        &mut self,
        app: &mut ComicReaderApp<egui::TextureHandle>,
        library_service: Option<&LibraryService>,
        comic_ids: &[i64],
    ) {
        let Some(service) = library_service else {
            app.library.status_text = Some("Library database unavailable".to_owned());
            return;
        };
        if comic_ids.is_empty() {
            return;
        }
        let id_set = comic_ids.iter().copied().collect::<HashSet<_>>();

        let targets = app
            .library
            .items
            .iter()
            .filter(|item| id_set.contains(&item.comic_id))
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
                        .pop(thumbnail.to_string_lossy().as_ref());
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
        app.library.refresh_filter_cache();
        app.library.status_text = Some(error_text.unwrap_or_else(|| {
            format!(
                "Removed {removed} comic{}",
                if removed == 1 { "" } else { "s" }
            )
        }));
    }

    /// Drops a cover that is recorded as ready but cannot be read back, so the
    /// scheduler regenerates it. Without this the entry is terminal: the row
    /// keeps reporting Ready across restarts and every render fails again.
    fn discard_unreadable_cover(
        &mut self,
        app: &mut ComicReaderApp<egui::TextureHandle>,
        library_service: Option<&LibraryService>,
        comic_id: i64,
    ) {
        let Some(item) = app
            .library
            .items
            .iter_mut()
            .find(|item| item.comic_id == comic_id)
        else {
            return;
        };

        let cache_path = cache_path_for_source(
            &self.thumbnail_cache_root,
            &item.path,
            &item.source_fingerprint,
        );
        let _ = std::fs::remove_file(&cache_path);
        self.thumbnail_textures
            .pop(cache_path.to_string_lossy().as_ref());
        if let Some(service) = library_service {
            let _ = service.set_thumbnail_key(&item.path, None);
        }
        item.thumbnail_status = ThumbnailStatus::Missing;
    }

    fn schedule_missing_thumbnails(
        &mut self,
        app: &mut ComicReaderApp<egui::TextureHandle>,
        library_service: Option<&LibraryService>,
    ) {
        let Some(pool) = &self.thumbnail_pool else {
            return;
        };

        let mut scheduled_this_frame = 0;
        let mut examined_this_frame = 0;
        for item in &mut app.library.items {
            // Two separate budgets. Handing work to a worker is the expensive
            // one and stays at two per frame; adopting a cover that is already
            // on disk only costs a stat, so rationing it at the same rate made
            // a large library take thousands of frames to show covers it had.
            if scheduled_this_frame >= 2 || examined_this_frame >= MAX_THUMBNAIL_STATS_PER_FRAME {
                break;
            }
            if !matches!(
                item.thumbnail_status,
                ThumbnailStatus::Missing | ThumbnailStatus::Stale
            ) || self.pending_thumbnails.contains(&item.path)
            {
                continue;
            }
            examined_this_frame += 1;

            let cache_path = cache_path_for_source(
                &self.thumbnail_cache_root,
                &item.path,
                &item.source_fingerprint,
            );
            if cache_path.exists() {
                let cache_path = cache_path.to_string_lossy().into_owned();
                if let Some(service) = library_service {
                    let _ = service.set_thumbnail_key(&item.path, Some(&cache_path));
                }
                item.thumbnail_status = ThumbnailStatus::Ready { cache_path };
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
                // A full queue is backpressure, not a failure: leave the item
                // Missing so it is retried once the workers drain, and stop
                // scheduling this frame rather than burning through the rest of
                // the library marking every comic as failed.
                Err(ThumbnailCacheError::QueueFull) => break,
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
    // Compare resolved paths, not lexical prefixes. A symlink inside the store
    // pointing outside it passes a `starts_with` check while the removal lands
    // somewhere else entirely.
    let (Ok(resolved), Ok(root)) = (Path::new(path).canonicalize(), store_root.canonicalize())
    else {
        return;
    };
    if !resolved.starts_with(&root) {
        return;
    }
    if std::fs::remove_file(&resolved).is_err() {
        return;
    }
    // Clean up the now-empty content-hash directory, but never the store root
    // itself, which is expected to outlive its contents.
    if let Some(parent) = resolved.parent()
        && parent != root
        && parent.starts_with(&root)
        && parent
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
    {
        let _ = std::fs::remove_dir(parent);
    }
}

fn import_status_text(
    added: usize,
    already_present: usize,
    failed: usize,
    error: Option<String>,
) -> String {
    if let Some(error) = error {
        return error;
    }
    let mut parts = vec![format!(
        "Added {added} comic{}",
        if added == 1 { "" } else { "s" }
    )];
    if already_present > 0 {
        parts.push(format!("{already_present} already present"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    parts.join(", ")
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
            writer: None,
            number: None,
            is_read: false,
            current_page: 0,
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

/// Directory the import dialogs should open in: the last used one, else cwd.
fn import_pick_directory(config: &AppConfig) -> PathBuf {
    config
        .last_import_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Remembers the directory containing the picked file or folder so the next
/// import dialog opens there.
fn remember_import_dir(config: &mut AppConfig, picked: &Path) {
    let dir = picked
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(picked);
    config.last_import_dir = Some(dir.to_path_buf());
}

fn render_library_menu_bar(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    controls: &mut LibraryRootControls,
    settings: &mut SettingsWindowState,
    config: &mut AppConfig,
    library_service: Option<&LibraryService>,
) {
    egui::TopBottomPanel::top("library_menu_bar")
        .frame(editor_toolbar_frame())
        .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    let add_files = ui
                        .add_enabled(!controls.is_importing(), egui::Button::new("Add Files…"))
                        .clicked();
                    if add_files {
                        ui.close_menu();
                        if let Some(files) = rfd::FileDialog::new()
                            .add_filter("Comics", &["cbz", "cbr", "pdf"])
                            .set_directory(import_pick_directory(config))
                            .pick_files()
                        {
                            if let Some(first) = files.first() {
                                remember_import_dir(config, first);
                            }
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
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_directory(import_pick_directory(config))
                            .pick_folder()
                        {
                            remember_import_dir(config, &folder);
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
                    // View, Shelf, and Select toggles live on the library toolbar;
                    // the menu keeps only actions that have no toolbar equivalent.
                    if ui
                        .add_enabled(
                            !controls.is_rescanning() && !controls.is_importing(),
                            egui::Button::new("Rescan library"),
                        )
                        .on_hover_text(
                            "Check the library store for comics that have been moved or deleted",
                        )
                        .clicked()
                    {
                        ui.close_menu();
                        controls.start_rescan();
                    }
                    ui.separator();
                    let has_unavailable = app
                        .library
                        .items
                        .iter()
                        .any(|item| item.availability == ComicAvailability::Unavailable);
                    if ui
                        .add_enabled(has_unavailable, egui::Button::new("Purge unavailable"))
                        .clicked()
                    {
                        ui.close_menu();
                        let purged = app.purge_unavailable_from_view();
                        app.library.status_text =
                            Some(format!("Purged {purged} unavailable comic(s)"));
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

                if controls.is_importing() {
                    ui.separator();
                    ui.spinner();
                    ui.label(egui::RichText::new("Importing…").color(EDITOR_TEXT_MUTED));
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

#[derive(Default)]
struct FolderNode<'a> {
    group: Option<&'a crate::library::LibraryGroup>,
    children: std::collections::BTreeMap<&'a str, FolderNode<'a>>,
}

fn render_folder_tree_node(
    ui: &mut egui::Ui,
    name: &str,
    node: &FolderNode,
    next_filter: &mut Option<ActiveLibraryFilter>,
) {
    if node.children.is_empty() {
        if let Some(group) = node.group {
            ui.selectable_value(
                next_filter,
                Some(ActiveLibraryFilter {
                    kind: group.kind,
                    key: group.key.clone(),
                }),
                name,
            );
        }
    } else {
        egui::CollapsingHeader::new(name)
            .default_open(false)
            .show(ui, |ui| {
                if let Some(group) = node.group {
                    ui.selectable_value(
                        next_filter,
                        Some(ActiveLibraryFilter {
                            kind: group.kind,
                            key: group.key.clone(),
                        }),
                        "All in folder",
                    );
                }
                for (child_name, child_node) in &node.children {
                    render_folder_tree_node(ui, child_name, child_node, next_filter);
                }
            });
    }
}

fn render_folder_tree(
    ui: &mut egui::Ui,
    folders: &[&crate::library::LibraryGroup],
    next_filter: &mut Option<ActiveLibraryFilter>,
) {
    let mut root = FolderNode::default();
    for folder in folders {
        let parts: Vec<&str> = folder.key.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut root;
        for part in parts {
            current = current.children.entry(part).or_default();
        }
        current.group = Some(folder);
    }
    for (name, node) in &root.children {
        render_folder_tree_node(ui, name, node, next_filter);
    }
}

fn render_library_shelf(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    controls: &mut LibraryRootControls,
) {
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
            let mut next_filter = app.library.active_filter.clone();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.selectable_value(&mut next_filter, None, "All comics");

                let groups = app.library.groups();
                let builtins = groups
                    .iter()
                    .filter(|g| g.kind == LibraryGroupKind::Builtin);
                for group in builtins {
                    ui.selectable_value(
                        &mut next_filter,
                        Some(ActiveLibraryFilter {
                            kind: group.kind,
                            key: group.key.clone(),
                        }),
                        group_label(group),
                    );
                }

                let series = groups
                    .iter()
                    .filter(|g| g.kind == LibraryGroupKind::Series)
                    .collect::<Vec<_>>();
                if !series.is_empty() {
                    ui.separator();
                    egui::CollapsingHeader::new("Series")
                        .default_open(true)
                        .show(ui, |ui| {
                            for group in series {
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
                }

                let folders = groups
                    .iter()
                    .filter(|g| g.kind == LibraryGroupKind::Folder)
                    .collect::<Vec<_>>();
                if !folders.is_empty() {
                    ui.separator();
                    egui::CollapsingHeader::new("Folders")
                        .default_open(true)
                        .show(ui, |ui| {
                            // Render folder tree
                            render_folder_tree(ui, &folders, &mut next_filter);
                        });
                }
            });

            if next_filter != app.library.active_filter {
                app.library.active_filter = next_filter;
                app.library.refresh_filter_cache();
            }
        });
    controls.shelf_open = shelf_open;
}

fn render_library_status(ui: &mut egui::Ui, app: &mut ComicReaderApp<egui::TextureHandle>) {
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
            ui.label(egui::RichText::new("A CBR/CBZ/PDF comic reader.").color(EDITOR_TEXT_MUTED));
            ui.add_space(6.0);
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
        });
}

/// Reader keyboard shortcuts grouped for the Help reference window. Keep in sync
/// with `handle_keybindings` in src/viewer/ui.rs and the README.
const SHORTCUT_GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "Navigation",
        &[
            ("\u{2192}  /  Page Down", "Next page (RTL: previous)"),
            ("\u{2190}  /  Page Up", "Previous page (RTL: next)"),
            ("Space  /  \u{2193}", "Scroll down"),
            ("\u{2191}", "Scroll up"),
            ("Home", "First page"),
            ("End", "Last page"),
            ("Esc", "Close window / back to library"),
        ],
    ),
    (
        "View and zoom",
        &[
            ("F", "Fit to window"),
            ("Shift + F", "Fill window"),
            ("W", "Fit width"),
            ("H", "Fit height"),
            ("1", "Actual size (1:1)"),
            ("+  /  =", "Zoom in"),
            ("-", "Zoom out"),
            ("S", "Toggle two-page spread"),
            ("V", "Toggle continuous scroll"),
            ("R", "Rotate right"),
            ("Shift + R", "Rotate left"),
        ],
    ),
    (
        "Panels and window",
        &[
            ("I", "Comic info panel"),
            ("Tab", "Hide / show toolbars and sidebar"),
            ("F11", "Toggle fullscreen"),
        ],
    ),
    (
        "Mouse",
        &[
            (
                "Click left / right side",
                "Previous / next page (direction-aware)",
            ),
            ("Scroll wheel", "Turn pages at fit zoom, pan when zoomed"),
            ("Pinch  /  Ctrl + scroll", "Zoom at pointer"),
            ("Drag", "Pan a zoomed page"),
            ("Double-click center", "Reset zoom"),
        ],
    ),
    ("Bookmarks", &[("B", "Toggle bookmark on current page")]),
];

fn render_shortcuts_window(ctx: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    egui::Window::new("Keyboard Shortcuts")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Shortcuts apply while reading a comic.")
                    .color(EDITOR_TEXT_MUTED),
            );
            ui.add_space(8.0);
            for (group, entries) in SHORTCUT_GROUPS {
                ui.label(egui::RichText::new(*group).color(EDITOR_GREEN).strong());
                egui::Grid::new(format!("shortcut_grid_{group}"))
                    .num_columns(2)
                    .spacing(egui::vec2(18.0, 4.0))
                    .show(ui, |ui| {
                        for (keys, action) in *entries {
                            ui.label(egui::RichText::new(*keys).monospace().color(EDITOR_CYAN));
                            ui.label(*action);
                            ui.end_row();
                        }
                    });
                ui.add_space(8.0);
            }
        });
}

fn render_library_toolbar(
    ctx: &egui::Context,
    app: &mut ComicReaderApp<egui::TextureHandle>,
    controls: &mut LibraryRootControls,
    config: &mut AppConfig,
) {
    egui::TopBottomPanel::top("library_toolbar")
        .frame(editor_toolbar_frame())
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                ui.spacing_mut().item_spacing.x = 4.0;
                let importing = controls.is_importing();
                if icon_button_enabled(ui, !importing, icon::FILE_PLUS, "Add comic files\u{2026}")
                    .clicked()
                    && let Some(files) = rfd::FileDialog::new()
                        .add_filter("Comics", &["cbz", "cbr", "pdf"])
                        .set_directory(import_pick_directory(config))
                        .pick_files()
                {
                    if let Some(first) = files.first() {
                        remember_import_dir(config, first);
                    }
                    controls.start_import_files(files);
                }
                if icon_button_enabled(ui, !importing, icon::FOLDER_PLUS, "Add a folder\u{2026}")
                    .clicked()
                    && let Some(folder) = rfd::FileDialog::new()
                        .set_directory(import_pick_directory(config))
                        .pick_folder()
                {
                    remember_import_dir(config, &folder);
                    controls.start_import_folder(folder);
                }

                ui.separator();
                let is_thumbnails = app.library.view_mode == LibraryViewMode::Thumbnails;
                if icon_toggle(ui, is_thumbnails, icon::SQUARES_FOUR, "Thumbnail view").clicked() {
                    app.library.view_mode = LibraryViewMode::Thumbnails;
                }
                if icon_toggle(ui, !is_thumbnails, icon::LIST, "List view").clicked() {
                    app.library.view_mode = LibraryViewMode::List;
                }

                ui.separator();
                if icon_toggle(
                    ui,
                    controls.shelf_open,
                    icon::SIDEBAR_SIMPLE,
                    "Toggle shelf",
                )
                .clicked()
                {
                    controls.shelf_open = !controls.shelf_open;
                }
                let mut select_mode = app.library.select_mode;
                if icon_toggle(
                    ui,
                    select_mode,
                    icon::CHECK_SQUARE,
                    "Select multiple comics",
                )
                .clicked()
                {
                    select_mode = !select_mode;
                    app.library.select_mode = select_mode;
                    if !select_mode {
                        app.library.selected_ids.clear();
                    }
                }

                ui.separator();
                render_library_filter_controls(ui, app);

                if controls.is_importing() {
                    ui.separator();
                    ui.spinner();
                    ui.label(egui::RichText::new("Importing\u{2026}").color(EDITOR_TEXT_MUTED));
                }
            });
        });
}

fn render_library_filter_controls(
    ui: &mut egui::Ui,
    app: &mut ComicReaderApp<egui::TextureHandle>,
) {
    if app.library.groups().is_empty() {
        app.library.active_filter = None;
        return;
    }
    let mut next_filter = app.library.active_filter.clone();
    let selected_label = app
        .library
        .active_filter
        .as_ref()
        .and_then(|active| {
            app.library
                .groups()
                .iter()
                .find(|group| group.kind == active.kind && group.key == active.key)
        })
        .map(group_label)
        .unwrap_or_else(|| "All comics".to_owned());

    ui.horizontal_wrapped(|ui| {
        ui.label(icon_text(icon::FUNNEL).color(EDITOR_GREEN))
            .on_hover_text("Filter by series or folder");

        egui::ComboBox::from_id_salt("library_filter")
            .selected_text(selected_label)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut next_filter, None, "All comics");
                for group in app
                    .library
                    .groups()
                    .iter()
                    .filter(|group| group.kind == LibraryGroupKind::Builtin)
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
                for group in app
                    .library
                    .groups()
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
                for group in app
                    .library
                    .groups()
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

        ui.add_space(16.0);
        ui.label(icon_text(icon::MAGNIFYING_GLASS).color(EDITOR_GREEN));
        let mut query = app.library.search_query.clone();
        let search_response = ui.text_edit_singleline(&mut query);
        if search_response.changed() {
            app.library.search_query = query;
            app.library.refresh_filter_cache();
        }

        ui.add_space(16.0);
        ui.label("Sort by:");
        let mut sort_option = app.library.sort_option;
        egui::ComboBox::from_id_salt("library_sort")
            .selected_text(match sort_option {
                crate::app::LibrarySortOption::Title => "Title",
                crate::app::LibrarySortOption::DateAdded => "Date Added",
                crate::app::LibrarySortOption::Series => "Series",
                crate::app::LibrarySortOption::Number => "Number",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut sort_option,
                    crate::app::LibrarySortOption::Title,
                    "Title",
                );
                ui.selectable_value(
                    &mut sort_option,
                    crate::app::LibrarySortOption::DateAdded,
                    "Date Added",
                );
                ui.selectable_value(
                    &mut sort_option,
                    crate::app::LibrarySortOption::Series,
                    "Series",
                );
                ui.selectable_value(
                    &mut sort_option,
                    crate::app::LibrarySortOption::Number,
                    "Number",
                );
            });
        if sort_option != app.library.sort_option {
            app.library.sort_option = sort_option;
            app.library.refresh_filter_cache();
        }
    });

    if next_filter != app.library.active_filter {
        app.library.active_filter = next_filter;
        app.library.refresh_filter_cache();
    }
}

fn group_label(group: &crate::library::LibraryGroup) -> String {
    let prefix = match group.kind {
        LibraryGroupKind::Builtin => "Shelf",
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
    thumbnail_textures: &mut lru::LruCache<String, egui::TextureHandle>,
) -> Option<LibraryItemEvent> {
    let status = match &item.thumbnail_status {
        ThumbnailStatus::Missing => "No cover",
        ThumbnailStatus::Loading => "Loading cover",
        ThumbnailStatus::Ready { .. } => "Cover ready",
        ThumbnailStatus::Failed { .. } => "Cover failed",
        ThumbnailStatus::Stale => "Cover stale",
    };

    ui.vertical(|ui| {
        ui.set_width(GRID_TILE_WIDTH);
        // Constant tile height keeps every grid row uniform, which the
        // virtualized scroll area's row estimate depends on.
        ui.set_height(GRID_TILE_HEIGHT);
        let mut event: Option<LibraryItemEvent> = None;
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
                    // The cache file is gone or corrupt. Discard the entry so
                    // the cover is regenerated instead of failing forever.
                    event = Some(LibraryItemEvent::CoverUnreadable {
                        comic_id: item.comic_id,
                    });
                    placeholder_cover_button(ui, thumbnail_size, "Cover failed")
                }
            }
            _ => placeholder_cover_button(ui, thumbnail_size, status),
        };
        // Truncated text is unreadable without a tooltip carrying the rest.
        // The tile fits roughly 22 monospace characters per line.
        let truncated = item.title.chars().count() > 22
            || item
                .subtitle
                .as_ref()
                .is_some_and(|subtitle| subtitle.chars().count() > 22);
        let response = if truncated {
            let mut hover = item.title.clone();
            if let Some(subtitle) = &item.subtitle {
                hover.push('\n');
                hover.push_str(subtitle);
            }
            response.on_hover_text(hover)
        } else {
            response
        };
        if is_selected {
            ui.painter().rect_stroke(
                response.rect,
                4.0,
                egui::Stroke::new(2.0, EDITOR_GREEN),
                egui::StrokeKind::Inside,
            );
        }
        let title_color = if is_selected {
            EDITOR_GREEN
        } else {
            EDITOR_TEXT
        };
        // Single-line truncated labels keep the tile height constant.
        ui.add(egui::Label::new(egui::RichText::new(&item.title).color(title_color)).truncate());
        if let Some(subtitle) = &item.subtitle {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(subtitle)
                        .color(EDITOR_TEXT_MUTED)
                        .small(),
                )
                .truncate(),
            );
        }
        if item.is_read {
            ui.label(egui::RichText::new("✓ Read").color(EDITOR_GREEN).small());
        } else if item.current_page > 0 && item.page_count > 0 {
            let progress = item.current_page as f32 / item.page_count as f32;
            ui.add(egui::ProgressBar::new(progress).desired_width(GRID_TILE_WIDTH));
        } else {
            ui.label(
                egui::RichText::new("Unread")
                    .color(EDITOR_TEXT_MUTED)
                    .small(),
            );
        }
        if item.is_read {
            paint_read_badge(ui, response.rect);
        }
        response.context_menu(|ui| {
            let label = if item.is_read {
                "Mark as unread"
            } else {
                "Mark as read"
            };
            if ui.button(label).clicked() {
                ui.close_menu();
                event = Some(LibraryItemEvent::SetRead {
                    comic_id: item.comic_id,
                    is_read: !item.is_read,
                });
            }
            if ui.button("Open file location").clicked() {
                ui.close_menu();
                open_file_location(&item.path);
            }
            ui.separator();
            if ui.button("Remove from library").clicked() {
                ui.close_menu();
                event = Some(LibraryItemEvent::Remove {
                    comic_id: item.comic_id,
                });
            }
        });
        if response.clicked() {
            event = Some(LibraryItemEvent::Open(Box::new(item.clone())));
        }
        event
    })
    .inner
}

fn paint_read_badge(ui: &egui::Ui, rect: egui::Rect) {
    let pad = 6.0;
    let badge_size = egui::vec2(54.0, 20.0);
    let badge_rect = egui::Rect::from_min_size(
        egui::pos2(rect.right() - badge_size.x - pad, rect.top() + pad),
        badge_size,
    );
    ui.painter().rect_filled(badge_rect, 3.0, EDITOR_GREEN);
    ui.painter().text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        "✓ READ",
        egui::FontId::proportional(12.0),
        EDITOR_PANEL_DARK,
    );
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
    thumbnail_textures: &mut lru::LruCache<String, egui::TextureHandle>,
) -> Option<LibraryItemEvent> {
    let row_height = LIST_THUMBNAIL_HEIGHT + 8.0;
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
    let mut event: Option<LibraryItemEvent> = None;
    if paint_list_thumbnail(ui, item, thumbnail_textures, thumb_rect) {
        event = Some(LibraryItemEvent::CoverUnreadable {
            comic_id: item.comic_id,
        });
    }

    let text_x = thumb_rect.right() + 12.0;
    let title_pos = egui::pos2(text_x, rect.top() + 12.0);
    let subtitle_pos = egui::pos2(text_x, rect.top() + 33.0);
    let meta_pos = egui::pos2(text_x, rect.top() + 53.0);
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
    let mut meta = format!(
        "{} page{}",
        item.page_count,
        if item.page_count == 1 { "" } else { "s" }
    );
    if let Some(series) = &item.series {
        meta.push_str("  |  ");
        meta.push_str(series);
    }
    if let Some(writer) = &item.writer {
        meta.push_str("  |  ");
        meta.push_str(writer);
    }
    painter.text(
        meta_pos,
        egui::Align2::LEFT_TOP,
        ellipsize_text(&meta, 100),
        egui::FontId::monospace(12.0),
        EDITOR_CYAN,
    );

    if item.is_read {
        painter.text(
            egui::pos2(rect.right() - 12.0, rect.top() + 12.0),
            egui::Align2::RIGHT_TOP,
            "✓ READ",
            egui::FontId::proportional(12.0),
            EDITOR_GREEN,
        );
    } else if item.current_page > 0 && item.page_count > 0 {
        let percent = (item.current_page as f32 / item.page_count as f32 * 100.0).round();
        painter.text(
            egui::pos2(rect.right() - 12.0, rect.top() + 12.0),
            egui::Align2::RIGHT_TOP,
            format!("{percent:.0}%"),
            egui::FontId::proportional(12.0),
            EDITOR_ORANGE,
        );
    }

    let response = if item.title.chars().count() > 80 {
        response.on_hover_text(item.title.clone())
    } else {
        response
    };

    response.context_menu(|ui| {
        let label = if item.is_read {
            "Mark as unread"
        } else {
            "Mark as read"
        };
        if ui.button(label).clicked() {
            ui.close_menu();
            event = Some(LibraryItemEvent::SetRead {
                comic_id: item.comic_id,
                is_read: !item.is_read,
            });
        }
        if ui.button("Open file location").clicked() {
            ui.close_menu();
            open_file_location(&item.path);
        }
        ui.separator();
        if ui.button("Remove from library").clicked() {
            ui.close_menu();
            event = Some(LibraryItemEvent::Remove {
                comic_id: item.comic_id,
            });
        }
    });
    if response.clicked() {
        event = Some(LibraryItemEvent::Open(Box::new(item.clone())));
    }
    event
}

/// Paints the row's cover. Returns true when a cover recorded as Ready could
/// not be read back, so the caller can discard and regenerate it.
fn paint_list_thumbnail(
    ui: &mut egui::Ui,
    item: &LibraryGridItem,
    thumbnail_textures: &mut lru::LruCache<String, egui::TextureHandle>,
    rect: egui::Rect,
) -> bool {
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
                return false;
            }
            paint_thumbnail_label(ui, rect, "Failed");
            return true;
        }
        ThumbnailStatus::Loading => paint_thumbnail_label(ui, rect, "Loading"),
        ThumbnailStatus::Failed { .. } => paint_thumbnail_label(ui, rect, "Failed"),
        ThumbnailStatus::Missing | ThumbnailStatus::Stale => {
            paint_thumbnail_label(ui, rect, "Cover")
        }
    }
    false
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

pub(crate) fn fit_image_size(source: egui::Vec2, bounds: egui::Vec2) -> egui::Vec2 {
    if source.x <= 0.0 || source.y <= 0.0 {
        return bounds;
    }
    let scale = (bounds.x / source.x).min(bounds.y / source.y);
    source * scale
}

fn open_file_location(path: &str) {
    // Resolve first. The value comes from the database, and on Windows it is
    // interpolated into a single /select, argument that CommandLineToArgvW
    // re-parses, so a path containing quotes could change how the argument is
    // split. Canonicalizing also drops the request if the file is gone.
    let Ok(path) = Path::new(path).canonicalize() else {
        return;
    };
    let path = path.as_path();
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "windows")]
    {
        // Reject anything that could break out of the single composed
        // argument rather than passing it to the shell's parser.
        if path
            .as_os_str()
            .to_string_lossy()
            .contains(['"', '\r', '\n'])
        {
            return;
        }
        let arg = format!("/select,{}", path.display());
        let _ = std::process::Command::new("explorer").arg(arg).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = path.parent().unwrap_or(path);
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
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
    /// Latest unsaved reading position. Held so the position can still be
    /// written after the session is torn down (returning to the library).
    pending_progress: Option<ProgressSnapshot>,
    /// Time (egui clock, seconds) at which `pending_progress` should be
    /// written. `None` means nothing is waiting.
    progress_flush_due_at: Option<f64>,
    /// Theme the egui style was last built for, so it is only rebuilt on a
    /// change rather than every frame.
    last_applied_dark_mode: Option<bool>,
    last_window_title: Option<String>,
}

/// How long a reading position is held in memory before being written. Each
/// write is a synchronous SQLite commit, so persisting on every page change
/// puts an fsync in the middle of continuous scrolling.
const PROGRESS_FLUSH_INTERVAL_S: f64 = 2.0;

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
            .map_err(|error| error.to_string())
            .and_then(|_| {
                LibraryService::initialize(&library_db_path).map_err(|error| error.to_string())
            });
        // Running without a database is a heavily degraded mode where every
        // write silently does nothing, so keep the reason and show it instead
        // of the generic "database unavailable" the write paths report.
        let (library_service, open_error) = match library_service {
            Ok(service) => (Some(service), None),
            Err(error) => (
                None,
                Some(format!(
                    "Library database unavailable ({}): {error}",
                    library_db_path.display()
                )),
            ),
        };

        let mut app = Self {
            inner: ComicReaderApp::default(),
            library_controls: LibraryRootControls::default(),
            config,
            config_path,
            library_service,
            settings: SettingsWindowState::default(),
            last_checkpointed_progress: None,
            pending_progress: None,
            progress_flush_due_at: None,
            last_applied_dark_mode: None,
            last_window_title: None,
        };
        app.hydrate_library_from_service();
        if app.config.resume_last_session {
            app.resume_last_session_from_service();
        }
        app.apply_config_to_active_session();
        if let Some(error) = open_error {
            app.record_lifecycle_error(error);
        }
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
            pending_progress: None,
            progress_flush_due_at: None,
            last_applied_dark_mode: None,
            last_window_title: None,
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
            // The session is already gone, so the throttled checkpoint can no
            // longer see its position. Write whatever it last observed before
            // dropping the resume state.
            self.flush_pending_progress();
            self.config.resume_last_session = false;
            self.last_checkpointed_progress = None;
            if let Err(error) = self.config.save(&self.config_path) {
                self.record_lifecycle_error(format!("Config save failed: {error}"));
            }
        }
    }

    pub fn flush_lifecycle_state(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        // Prefer the live session; fall back to a position that was still
        // waiting on the flush timer when the session ended.
        let active_snapshot = self
            .inner
            .active_progress_snapshot()
            .or(self.pending_progress);
        self.pending_progress = None;
        self.progress_flush_due_at = None;
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

    /// Writes the active reading position immediately, skipping the flush
    /// timer. Used by callers that need the position durable right away.
    pub fn checkpoint_active_progress(&mut self) -> Result<(), String> {
        let Some(snapshot) = self.inner.active_progress_snapshot() else {
            return Ok(());
        };
        self.pending_progress = None;
        self.progress_flush_due_at = None;
        self.write_progress_snapshot(snapshot)
    }

    /// Records the active reading position and writes it once it has been
    /// stable for `PROGRESS_FLUSH_INTERVAL_S`. Continuous scrolling changes the
    /// current page many times per second and each write is a synchronous
    /// SQLite commit, so writing on every change stalls the frame loop.
    fn checkpoint_active_progress_throttled(&mut self, ctx: &egui::Context) {
        let Some(snapshot) = self.inner.active_progress_snapshot() else {
            self.pending_progress = None;
            self.progress_flush_due_at = None;
            return;
        };
        if self.last_checkpointed_progress == Some(snapshot) {
            self.pending_progress = None;
            self.progress_flush_due_at = None;
            return;
        }

        self.pending_progress = Some(snapshot);
        let now = ctx.input(|input| input.time);
        let due_at = *self
            .progress_flush_due_at
            .get_or_insert(now + PROGRESS_FLUSH_INTERVAL_S);
        if now >= due_at {
            self.flush_pending_progress();
        } else {
            ctx.request_repaint_after(Duration::from_secs_f64(due_at - now));
        }
    }

    fn flush_pending_progress(&mut self) {
        self.progress_flush_due_at = None;
        let Some(snapshot) = self.pending_progress.take() else {
            return;
        };
        let _ = self.write_progress_snapshot(snapshot);
    }

    fn write_progress_snapshot(&mut self, snapshot: ProgressSnapshot) -> Result<(), String> {
        if self.last_checkpointed_progress == Some(snapshot) {
            return Ok(());
        }
        let Some(service) = &self.library_service else {
            return Ok(());
        };

        match service.save_progress(snapshot.comic_id, snapshot.current_page, snapshot.is_read) {
            Ok(_) => {
                if let Some(item) = self
                    .inner
                    .library
                    .items
                    .iter_mut()
                    .find(|item| item.comic_id == snapshot.comic_id)
                {
                    item.current_page = snapshot.current_page;
                    item.is_read = snapshot.is_read;
                }
                self.inner.library.refresh_progress_derived_state();
                // Persist the resume flag to disk on the first checkpoint so a hard exit
                // (no save hook) still reopens this comic on next launch.
                let needs_config_save = !self.config.resume_last_session;
                self.config.resume_last_session = true;
                self.last_checkpointed_progress = Some(snapshot);
                if needs_config_save && let Err(error) = self.config.save(&self.config_path) {
                    let message = format!("Config save failed: {error}");
                    self.record_lifecycle_error(message.clone());
                    return Err(message);
                }
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

    /// Shows the active comic and reading position in the window title.
    fn sync_window_title(&mut self, ctx: &egui::Context) {
        let title = match (&self.inner.state, &self.inner.reading) {
            (AppState::Reading(comic_id), Some(session)) => {
                let name = self
                    .inner
                    .library
                    .items
                    .iter()
                    .find(|item| item.comic_id == *comic_id)
                    .map(|item| item.title.as_str())
                    .unwrap_or("Reading");
                format!(
                    "{} - {}/{} - cbr-egui",
                    name,
                    session.current_page_index + 1,
                    session.page_count
                )
            }
            _ => "cbr-egui".to_owned(),
        };
        if self.last_window_title.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_window_title = Some(title);
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
        // Only dark_mode feeds the style, so rebuilding and re-uploading the
        // whole egui Style every frame achieved nothing.
        if self.last_applied_dark_mode != Some(self.config.dark_mode) {
            self.apply_config_to_context(ctx);
            self.last_applied_dark_mode = Some(self.config.dark_mode);
        }
        let was_reading = matches!(self.inner.state, AppState::Reading(_));
        route_app_update(
            ctx,
            &mut self.inner,
            &mut self.library_controls,
            &mut self.settings,
            &mut self.config,
            self.library_service.as_ref(),
        );
        self.reconcile_resume_state_after_route(was_reading);
        self.checkpoint_active_progress_throttled(ctx);
        self.sync_window_title(ctx);
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
            pending_progress: None,
            progress_flush_due_at: None,
            last_applied_dark_mode: None,
            last_window_title: None,
        }
    }
}
