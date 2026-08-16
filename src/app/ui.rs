// ABOUTME: Renders the egui application shell, library controls, and reader toolbar.
// ABOUTME: Connects UI events to the app state without owning archive or storage logic.
use std::path::PathBuf;
use std::time::Duration;

use eframe::egui;

use crate::app::library_view::{
    open_grid_item_in_reader, render_about_window, render_library_menu_bar, render_library_shelf,
    render_library_status, render_library_toolbar, render_shortcuts_window,
};
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
    EDITOR_GREEN, apply_editor_text_styles, editor_dark_visuals, editor_panel_frame,
};
use crate::app::{AppState, ComicReaderApp, LibraryViewMode, ProgressSnapshot};

pub use crate::app::controls::{
    LibraryRootControls, SettingsWindowState, persist_scanned_comics_to_grid_items,
    scanned_comics_to_grid_items,
};
pub use crate::app::library_view::{
    GRID_GAP, GRID_TILE_HEIGHT, GRID_TILE_WIDTH, LibraryItemEvent, empty_library_text,
    render_library_grid, render_library_list, responsive_grid_columns,
};
pub use crate::app::theme::install_icon_fonts;
use crate::config::{AppConfig, default_config_path, default_library_db_path};
use crate::library::LibraryService;
use crate::viewer::{self, ReadingDirection};

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

pub(crate) fn fit_image_size(source: egui::Vec2, bounds: egui::Vec2) -> egui::Vec2 {
    if source.x <= 0.0 || source.y <= 0.0 {
        return bounds;
    }
    let scale = (bounds.x / source.x).min(bounds.y / source.y);
    source * scale
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
