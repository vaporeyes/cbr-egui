// ABOUTME: Renders the library: grid and list views, tiles, shelf, and toolbars.
// ABOUTME: Reports user intent back to the shell as LibraryItemEvent values.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use eframe::egui;
use egui_phosphor::regular as icon;

use crate::app::controls::SettingsWindowState;
use crate::app::controls::{LibraryRootControls, import_pick_directory, remember_import_dir};
use crate::app::reader::load_reader_page;
use crate::app::theme::{
    EDITOR_CYAN, EDITOR_GREEN, EDITOR_ORANGE, EDITOR_PANEL, EDITOR_PANEL_ACTIVE, EDITOR_PANEL_DARK,
    EDITOR_PURPLE, EDITOR_TEXT, EDITOR_TEXT_MUTED, EDITOR_WIDGET_HOVER, editor_toolbar_frame,
    icon_button_enabled, icon_text, icon_toggle,
};
use crate::app::ui::fit_image_size;
use crate::app::{ComicReaderApp, LibraryViewMode};
use crate::config::AppConfig;
use crate::library::{
    ActiveLibraryFilter, ComicAvailability, LibraryGridItem, LibraryGroupKind, LibraryService,
    SUPPORTED_COMIC_EXTENSIONS, ThumbnailStatus,
};

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

pub(crate) fn open_grid_item_in_reader(
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

pub(crate) fn default_thumbnail_cache_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".cache")
        .join("cbr-egui")
        .join("thumbnails")
}

pub(crate) fn color_image_from_file(path: &str) -> Result<egui::ColorImage, String> {
    let image = image::open(path).map_err(|err| err.to_string())?.to_rgba8();
    let (width, height) = image.dimensions();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        image.as_raw(),
    ))
}

pub(crate) fn load_thumbnail_texture(
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

pub(crate) fn render_library_menu_bar(
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
                            .add_filter("Comics", SUPPORTED_COMIC_EXTENSIONS)
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

pub(crate) fn render_library_shelf(
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

pub(crate) fn render_library_status(
    ui: &mut egui::Ui,
    app: &mut ComicReaderApp<egui::TextureHandle>,
) {
    if let Some(status_text) = &app.library.status_text {
        ui.label(egui::RichText::new(status_text).color(EDITOR_TEXT_MUTED));
    }
}

pub(crate) fn render_about_window(ctx: &egui::Context, open: &mut bool) {
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
pub(crate) const SHORTCUT_GROUPS: &[(&str, &[(&str, &str)])] = &[
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

pub(crate) fn render_shortcuts_window(ctx: &egui::Context, open: &mut bool) {
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

pub(crate) fn render_library_toolbar(
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
                        .add_filter("Comics", SUPPORTED_COMIC_EXTENSIONS)
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

pub(crate) fn open_file_location(path: &str) {
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

pub(crate) fn render_empty_library(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.heading(EMPTY_LIBRARY_TITLE);
            ui.add_space(8.0);
            ui.label(egui::RichText::new(EMPTY_LIBRARY_DETAIL).weak());
        });
    });
}
