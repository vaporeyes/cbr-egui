// ABOUTME: Owns the background import, rescan, and cover thumbnail pipelines.
// ABOUTME: Holds the library-side worker channels and the managed store on disk.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::thread;

use crossbeam_channel::{Receiver, bounded};
use eframe::egui;

use crate::app::ComicReaderApp;
use crate::app::ui::{default_thumbnail_cache_root, open_grid_item_in_reader};
use crate::config::{AppConfig, default_library_store_root};
use crate::library::{
    ComicAvailability, ImportSummary, LibraryGridItem, LibraryService, ScannedComic,
    ThumbnailCacheError, ThumbnailRequest, ThumbnailStatus, ThumbnailWorkerPool,
    cache_path_for_source, discover_supported_archives, import_paths, scan_library_root,
};

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
    pub(crate) thumbnail_textures: lru::LruCache<String, egui::TextureHandle>,
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

    pub(crate) fn is_importing(&self) -> bool {
        self.import_result_receiver.is_some()
    }

    pub(crate) fn is_rescanning(&self) -> bool {
        self.rescan_result_receiver.is_some()
    }

    pub(crate) fn has_pending_thumbnails(&self) -> bool {
        !self.pending_thumbnails.is_empty()
    }

    /// Walks the managed store and reconciles it against the database. Comics
    /// whose stored copy has been removed behind the app's back become
    /// unavailable, and page counts and metadata are refreshed from the files
    /// that are still there.
    pub(crate) fn start_rescan(&mut self) {
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

    pub(crate) fn poll_rescan(
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

    pub(crate) fn start_import_files(&mut self, files: Vec<PathBuf>) {
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
    pub(crate) fn start_import_and_open(&mut self, files: Vec<PathBuf>) {
        if self.is_importing() || files.is_empty() {
            return;
        }
        self.open_first_after_import = true;
        self.start_import_files(files);
    }

    pub(crate) fn start_import_folder(&mut self, folder: PathBuf) {
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
    pub(crate) fn start_import_dropped(&mut self, paths: Vec<PathBuf>) {
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

    pub(crate) fn poll_import(
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

    pub(crate) fn poll_thumbnails(
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

    pub(crate) fn remove_selected(
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
    pub(crate) fn remove_comics(
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
    pub(crate) fn discard_unreadable_cover(
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

    pub(crate) fn schedule_missing_thumbnails(
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

pub(crate) fn import_status_text(
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
pub(crate) fn import_pick_directory(config: &AppConfig) -> PathBuf {
    config
        .last_import_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Remembers the directory containing the picked file or folder so the next
/// import dialog opens there.
pub(crate) fn remember_import_dir(config: &mut AppConfig, picked: &Path) {
    let dir = picked
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(picked);
    config.last_import_dir = Some(dir.to_path_buf());
}
