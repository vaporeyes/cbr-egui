// ABOUTME: Defines the application state machines for library browsing and reading.
// ABOUTME: Owns session-scoped caches, prefetch bookkeeping, and view state helpers.
use std::collections::{BTreeSet, HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use crate::cache::{PageTextureCache, PageTextureCacheError};
use crate::decode::{
    CancellationToken, DecodePurpose, DecodeRequestId, ImageAdjustments, Rotation, WorkerPool,
};
use crate::library::{
    ActiveLibraryFilter, ArchivePage, ComicAvailability, LibraryError, LibraryGridItem,
    LibraryGroup, LibraryGroupKind, LibraryService, ComicMetadataDisplay,
};
use crate::vfs::ArchiveReader;
use crate::viewer::{
    ContinuousScrollState, PageGeneration, PageId, PageStatus, PrefetchState, ReadingDirection,
    ViewerState, ZoomPanState,
};

pub mod ui;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AppState {
    #[default]
    Library,
    Reading(i64),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LibraryViewMode {
    #[default]
    Thumbnails,
    List,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LibrarySortOption {
    #[default]
    Title,
    DateAdded,
    Series,
    Number,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryViewState {
    pub items: Vec<LibraryGridItem>,
    pub selected_comic_id: Option<i64>,
    pub status_text: Option<String>,
    pub view_mode: LibraryViewMode,
    pub sort_option: LibrarySortOption,
    pub search_query: String,
    pub active_filter: Option<ActiveLibraryFilter>,
    pub select_mode: bool,
    pub selected_ids: HashSet<i64>,
    groups: Vec<LibraryGroup>,
    visible_indices: Vec<usize>,
}

pub struct CachedPage<T> {
    pub texture: T,
    pub pixel_size: crate::viewer::Size2,
}

pub const DEFAULT_RAW_PAGE_CACHE_CAPACITY: usize = 9;

pub struct ReadingArchiveCache {
    source_path: Option<PathBuf>,
    reader: Option<Box<dyn ArchiveReader>>,
    pages: Vec<ArchivePage>,
    raw_pages: lru::LruCache<usize, Vec<u8>>,
}

impl Default for ReadingArchiveCache {
    fn default() -> Self {
        Self {
            source_path: None,
            reader: None,
            pages: Vec::new(),
            raw_pages: lru::LruCache::new(
                NonZeroUsize::new(DEFAULT_RAW_PAGE_CACHE_CAPACITY).expect("non-zero"),
            ),
        }
    }
}

impl ReadingArchiveCache {
    pub fn is_for(&self, source_path: impl AsRef<Path>) -> bool {
        self.source_path.as_deref() == Some(source_path.as_ref())
    }

    pub fn reset(
        &mut self,
        source_path: impl AsRef<Path>,
        reader: Box<dyn ArchiveReader>,
        pages: Vec<ArchivePage>,
    ) {
        self.source_path = Some(source_path.as_ref().to_path_buf());
        self.reader = Some(reader);
        self.pages = pages;
        self.raw_pages.clear();
    }

    pub fn read_page(&mut self, page_index: usize) -> Result<Vec<u8>, String> {
        if let Some(bytes) = self.raw_pages.get(&page_index) {
            return Ok(bytes.clone());
        }
        let page = self
            .pages
            .get(page_index)
            .ok_or_else(|| format!("Page {} is not available", page_index + 1))?;
        let reader = self
            .reader
            .as_mut()
            .ok_or_else(|| "Archive reader is not initialized".to_owned())?;
        let bytes = reader
            .read_page(&page.path)
            .map_err(|err| err.to_string())?;
        self.raw_pages.push(page_index, bytes.clone());
        Ok(bytes)
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Returns the archive entry path for a page index, used to build decode
    /// requests that read the page bytes on a worker thread.
    pub fn page_entry_path(&self, page_index: usize) -> Option<String> {
        self.pages.get(page_index).map(|page| page.path.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressSnapshot {
    pub comic_id: i64,
    pub current_page: u32,
    pub is_read: bool,
}

#[derive(Debug, Clone)]
pub struct InFlightPrefetch {
    pub page_index: usize,
    pub request_id: DecodeRequestId,
    pub generation: PageGeneration,
    pub purpose: DecodePurpose,
    pub cancellation_token: CancellationToken,
}

#[derive(Debug, Clone)]
pub struct PrefetchRuntime {
    pub generation: PageGeneration,
    pub queued_pages: HashSet<usize>,
    pub in_flight: HashMap<usize, InFlightPrefetch>,
    pub failed_pages: HashMap<usize, String>,
    next_request_id: u64,
}

impl Default for PrefetchRuntime {
    fn default() -> Self {
        Self {
            generation: PageGeneration(0),
            queued_pages: HashSet::new(),
            in_flight: HashMap::new(),
            failed_pages: HashMap::new(),
            next_request_id: 1,
        }
    }
}

impl PrefetchRuntime {
    pub fn next_request_id(&mut self) -> DecodeRequestId {
        let request_id = DecodeRequestId(self.next_request_id);
        self.next_request_id = self.next_request_id.saturating_add(1);
        request_id
    }

    pub fn advance_generation(&mut self) {
        self.generation = PageGeneration(self.generation.0.saturating_add(1));
    }

    pub fn track_in_flight(
        &mut self,
        page_index: usize,
        request_id: DecodeRequestId,
        purpose: DecodePurpose,
        cancellation_token: CancellationToken,
    ) {
        self.queued_pages.remove(&page_index);
        self.in_flight.insert(
            page_index,
            InFlightPrefetch {
                page_index,
                request_id,
                generation: self.generation,
                purpose,
                cancellation_token,
            },
        );
    }

    pub fn prefetch_state(
        &self,
        page_count: usize,
        cached: impl IntoIterator<Item = usize>,
    ) -> PrefetchState {
        PrefetchState {
            page_count,
            cached: cached.into_iter().collect(),
            queued: self.queued_pages.clone(),
            in_flight: self.in_flight.keys().copied().collect(),
        }
    }

    pub fn complete_request(
        &mut self,
        request_id: DecodeRequestId,
        page_index: usize,
    ) -> Option<InFlightPrefetch> {
        let in_flight = self.in_flight.get(&page_index)?;
        if in_flight.request_id != request_id {
            return None;
        }
        self.in_flight.remove(&page_index)
    }

    pub fn complete_fresh_request(
        &mut self,
        request_id: DecodeRequestId,
        page_index: usize,
    ) -> Option<InFlightPrefetch> {
        let in_flight = self.complete_request(request_id, page_index)?;
        // Workers may finish after a page jump or explicit cancellation. Removing
        // the request is still useful, but only current-generation results may
        // become display cache entries.
        if in_flight.generation == self.generation && !in_flight.cancellation_token.is_cancelled() {
            Some(in_flight)
        } else {
            None
        }
    }

    pub fn record_failed_page(&mut self, page_index: usize, message: impl Into<String>) {
        self.failed_pages.insert(page_index, message.into());
    }

    pub fn cancel_stale(
        &mut self,
        current_page: usize,
        page_count: usize,
        cached: impl IntoIterator<Item = usize>,
    ) -> Vec<usize> {
        let cached = cached.into_iter().collect::<HashSet<_>>();
        // Keep the same nearby-page window as prefetch_candidates, but compute it
        // independently so already in-flight useful pages are not cancelled just
        // because prefetch_candidates would filter them out.
        let keep = [
            Some(current_page),
            current_page
                .checked_add(1)
                .filter(|page| *page < page_count),
            current_page
                .checked_add(2)
                .filter(|page| *page < page_count),
            current_page.checked_sub(1),
        ]
        .into_iter()
        .flatten()
        .filter(|page_index| !cached.contains(page_index))
        .collect::<HashSet<_>>();
        let stale_pages = self
            .in_flight
            .keys()
            .copied()
            .filter(|page_index| !keep.contains(page_index))
            .collect::<Vec<_>>();
        for page_index in &stale_pages {
            if let Some(in_flight) = self.in_flight.remove(page_index) {
                in_flight.cancellation_token.cancel();
            }
        }
        self.queued_pages
            .retain(|page_index| keep.contains(page_index));
        stale_pages
    }

    pub fn cancel_stale_except(
        &mut self,
        keep_pages: impl IntoIterator<Item = usize>,
        cached: impl IntoIterator<Item = usize>,
    ) -> Vec<usize> {
        let cached = cached.into_iter().collect::<HashSet<_>>();
        // Continuous scroll candidates come from viewport geometry rather than
        // the fixed paged prefetch formula, so callers supply the exact useful
        // set and every other in-flight request can be cancelled.
        let keep = keep_pages
            .into_iter()
            .filter(|page_index| !cached.contains(page_index))
            .collect::<HashSet<_>>();
        let stale_pages = self
            .in_flight
            .keys()
            .copied()
            .filter(|page_index| !keep.contains(page_index))
            .collect::<Vec<_>>();
        for page_index in &stale_pages {
            if let Some(in_flight) = self.in_flight.remove(page_index) {
                in_flight.cancellation_token.cancel();
            }
        }
        self.queued_pages
            .retain(|page_index| keep.contains(page_index));
        stale_pages
    }

    pub fn cancel_all(&mut self) {
        for (_, in_flight) in self.in_flight.drain() {
            in_flight.cancellation_token.cancel();
        }
        self.queued_pages.clear();
    }

    pub fn has_work(&self) -> bool {
        !self.queued_pages.is_empty() || !self.in_flight.is_empty()
    }
}

impl Drop for PrefetchRuntime {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

pub struct ReadingSession<T> {
    pub comic_id: i64,
    pub current_page_index: usize,
    pub page_count: usize,
    pub spread_mode_enabled: bool,
    pub viewer_state: ViewerState<T>,
    pub prefetch: PrefetchRuntime,
    pub texture_cache: PageTextureCache<CachedPage<T>>,
    pub decode_worker_pool: Option<WorkerPool>,
    pub continuous_scroll: ContinuousScrollState,
    /// Inputs of the last continuous canvas build (measurements version,
    /// texture cache version, viewport width bits, page count); rebuilds are
    /// skipped while these are unchanged.
    pub continuous_layout_stamp: Option<(u64, u64, u32, usize)>,
    pub archive_cache: ReadingArchiveCache,
    pub goto_input: String,
    pub bookmarks: BTreeSet<usize>,
    pub bookmarks_loaded: bool,
    pub rotation: Rotation,
    pub adjustments: ImageAdjustments,
    pub show_adjustments: bool,
    pub show_page_sidebar: bool,
    /// Last page the sidebar auto-scrolled to; the sidebar follows navigation
    /// only when this falls behind the current page.
    pub sidebar_tracked_page: Option<usize>,
    pub page_thumbnails: HashMap<usize, T>,
    pub pending_page_thumbnails: HashSet<usize>,
    pub failed_page_thumbnails: HashSet<usize>,
    pub page_thumbnail_pool: Option<WorkerPool>,
    /// Bumped whenever the sidebar thumbnails are invalidated. Sidebar decodes
    /// carry no cancellation token, so this is what tells a result that was
    /// already in flight apart from one for the current orientation.
    page_thumbnail_generation: u64,
    pub show_info_panel: bool,
    pub metadata: Option<ComicMetadataDisplay>,
}

impl<T> ReadingSession<T> {
    pub fn new(comic_id: i64, page_count: usize) -> Self {
        Self {
            comic_id,
            current_page_index: 0,
            page_count,
            spread_mode_enabled: false,
            viewer_state: ViewerState::new(),
            prefetch: PrefetchRuntime::default(),
            texture_cache: PageTextureCache::with_default_capacity(),
            decode_worker_pool: WorkerPool::start(2, 16).ok(),
            continuous_scroll: ContinuousScrollState::new(),
            continuous_layout_stamp: None,
            archive_cache: ReadingArchiveCache::default(),
            goto_input: String::new(),
            bookmarks: BTreeSet::new(),
            bookmarks_loaded: false,
            rotation: Rotation::None,
            adjustments: ImageAdjustments::default(),
            show_adjustments: false,
            show_page_sidebar: true,
            sidebar_tracked_page: None,
            page_thumbnails: HashMap::new(),
            pending_page_thumbnails: HashSet::new(),
            failed_page_thumbnails: HashSet::new(),
            page_thumbnail_pool: WorkerPool::start(1, 64).ok(),
            page_thumbnail_generation: 0,
            show_info_panel: false,
            metadata: None,
        }
    }

    pub fn invalidate_page_thumbnails(&mut self) {
        self.page_thumbnails.clear();
        self.pending_page_thumbnails.clear();
        self.failed_page_thumbnails.clear();
        self.page_thumbnail_generation = self.page_thumbnail_generation.saturating_add(1);
    }

    /// Packs the current thumbnail generation into a request id. Sidebar
    /// decodes cannot be cancelled, so results that were already in flight when
    /// the orientation changed have to be recognised on arrival instead.
    pub fn page_thumbnail_request_id(&self, page_index: usize) -> DecodeRequestId {
        DecodeRequestId((self.page_thumbnail_generation << 32) | (page_index as u64 & 0xFFFF_FFFF))
    }

    pub fn is_current_page_thumbnail(&self, request_id: DecodeRequestId) -> bool {
        request_id.0 >> 32 == self.page_thumbnail_generation
    }

    pub fn try_new(
        comic_id: i64,
        page_count: usize,
        cache_capacity: usize,
    ) -> Result<Self, PageTextureCacheError> {
        Ok(Self {
            comic_id,
            current_page_index: 0,
            page_count,
            spread_mode_enabled: false,
            viewer_state: ViewerState::new(),
            prefetch: PrefetchRuntime::default(),
            texture_cache: PageTextureCache::new(cache_capacity)?,
            decode_worker_pool: WorkerPool::start(2, 16).ok(),
            continuous_scroll: ContinuousScrollState::new(),
            continuous_layout_stamp: None,
            archive_cache: ReadingArchiveCache::default(),
            goto_input: String::new(),
            bookmarks: BTreeSet::new(),
            bookmarks_loaded: false,
            rotation: Rotation::None,
            adjustments: ImageAdjustments::default(),
            show_adjustments: false,
            show_page_sidebar: true,
            sidebar_tracked_page: None,
            page_thumbnails: HashMap::new(),
            pending_page_thumbnails: HashSet::new(),
            failed_page_thumbnails: HashSet::new(),
            page_thumbnail_pool: WorkerPool::start(1, 64).ok(),
            page_thumbnail_generation: 0,
            show_info_panel: false,
            metadata: None,
        })
    }

    pub fn set_current_page(&mut self, page_index: usize) {
        let next_page_index = page_index.min(self.page_count.saturating_sub(1));
        if self.current_page_index != next_page_index {
            self.prefetch.advance_generation();
        }
        self.current_page_index = next_page_index;
        self.viewer_state
            .set_current_page(PageId(self.current_page_index as u64));
    }

    /// Tracks the page the continuous-scroll viewport is on. Unlike
    /// `set_current_page` this must not advance the prefetch generation
    /// (which would drop in-flight decodes for the same scroll burst) and
    /// must not reset paged-mode zoom/pan state.
    pub fn sync_continuous_current_page(&mut self, page_index: usize) {
        self.current_page_index = page_index.min(self.page_count.saturating_sub(1));
    }

    /// Changes the page rotation and invalidates everything derived from the
    /// previous orientation: cached textures, in-flight prefetch work, and
    /// continuous-scroll measurements must all be rebuilt from the re-decode.
    pub fn set_rotation(&mut self, rotation: Rotation) {
        if self.rotation == rotation {
            return;
        }
        self.rotation = rotation;
        self.prefetch.cancel_all();
        self.prefetch.failed_pages.clear();
        self.prefetch.advance_generation();
        self.texture_cache.clear();
        self.continuous_scroll.reset();
        self.viewer_state.next_page_status = PageStatus::Empty;
        self.viewer_state.zoom_pan = ZoomPanState::default();
        self.invalidate_page_thumbnails();
    }

    /// Invalidates decoded textures so pages re-decode with the current image
    /// adjustments. Page dimensions are unchanged, so continuous measurements
    /// and zoom/pan are preserved.
    pub fn invalidate_for_adjustments(&mut self) {
        self.prefetch.cancel_all();
        self.prefetch.failed_pages.clear();
        self.prefetch.advance_generation();
        self.texture_cache.clear();
        self.viewer_state.next_page_status = PageStatus::Empty;
    }

    pub fn set_spread_mode_enabled(&mut self, enabled: bool) {
        self.spread_mode_enabled = enabled;
        self.viewer_state.set_spread_mode_enabled(enabled);
    }

    pub fn set_reading_direction(&mut self, direction: ReadingDirection) {
        self.viewer_state.set_reading_direction(direction);
    }

    pub fn progress_snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            comic_id: self.comic_id,
            current_page: self.current_page_index as u32,
            // A pageless comic is not a finished one: without the page_count
            // guard, `0 + 1 >= 0` reports it as read.
            is_read: self.page_count > 0 && self.current_page_index + 1 >= self.page_count,
        }
    }

    pub fn prefetch_state(&self) -> PrefetchState {
        self.prefetch
            .prefetch_state(self.page_count, self.texture_cache.keys())
    }
}

pub struct ComicReaderApp<T> {
    pub state: AppState,
    pub library: LibraryViewState,
    pub reading: Option<ReadingSession<T>>,
}

impl<T> Default for ComicReaderApp<T> {
    fn default() -> Self {
        Self {
            state: AppState::Library,
            library: LibraryViewState::default(),
            reading: None,
        }
    }
}

impl<T> ComicReaderApp<T> {
    pub fn open_comic(&mut self, comic_id: i64, page_count: usize) {
        self.open_comic_with_reading_direction(comic_id, page_count, ReadingDirection::LeftToRight);
    }

    pub fn open_comic_with_reading_direction(
        &mut self,
        comic_id: i64,
        page_count: usize,
        reading_direction: ReadingDirection,
    ) {
        if let Some(reading) = &mut self.reading {
            reading.prefetch.cancel_all();
        }
        self.state = AppState::Reading(comic_id);
        self.library.selected_comic_id = Some(comic_id);
        let mut session = ReadingSession::new(comic_id, page_count);
        session.set_reading_direction(reading_direction);
        self.reading = Some(session);
    }

    pub fn open_grid_item(&mut self, item: &LibraryGridItem) -> bool {
        if item.availability != ComicAvailability::Available {
            self.library.status_text = Some("Comic is unavailable".to_owned());
            return false;
        }
        // An archive with no decodable images has nothing to show, and opening
        // it would immediately checkpoint it as finished.
        if item.page_count == 0 {
            self.library.status_text = Some("Comic contains no readable pages".to_owned());
            return false;
        }

        self.open_comic(item.comic_id, item.page_count as usize);
        true
    }

    /// Opens a comic and restores the saved reading position, if any.
    pub fn open_grid_item_resuming(
        &mut self,
        service: &LibraryService,
        item: &LibraryGridItem,
    ) -> bool {
        if !self.open_grid_item(item) {
            return false;
        }
        self.hydrate_session_from_service(service, item.comic_id, item.page_count);
        true
    }

    /// Restores persisted session state (reading position, metadata,
    /// bookmarks) for the active session. Shared by library opens and the
    /// startup resume path so both show the same state.
    fn hydrate_session_from_service(
        &mut self,
        service: &LibraryService,
        comic_id: i64,
        page_count: u32,
    ) {
        if let Ok(Some(progress)) = service.get_progress(comic_id) {
            let last_index = page_count.saturating_sub(1) as usize;
            // A finished comic restarts from the beginning instead of
            // reopening on its last page.
            let target = if progress.is_read {
                0
            } else {
                (progress.current_page as usize).min(last_index)
            };
            if let Some(reading) = &mut self.reading {
                reading.set_current_page(target);
            }
        }
        if let Ok(Some(row)) = service.get_comic_row(comic_id)
            && let Some(reading) = &mut self.reading
        {
            reading.metadata = Some(row.metadata);
        }
        if let Some(reading) = &mut self.reading {
            match service.list_bookmarks(comic_id) {
                Ok(bookmarks) => {
                    reading.bookmarks = bookmarks
                        .into_iter()
                        .map(|bookmark| bookmark.page_index as usize)
                        .collect();
                }
                Err(error) => {
                    reading.viewer_state.chrome.status_text =
                        Some(format!("Bookmarks load failed: {error}"));
                }
            }
            reading.bookmarks_loaded = true;
        }
    }

    pub fn set_comic_read(
        &mut self,
        service: &LibraryService,
        comic_id: i64,
        is_read: bool,
    ) -> Result<(), LibraryError> {
        let current_page = service
            .get_progress(comic_id)?
            .map(|progress| progress.current_page)
            .unwrap_or(0);
        service.save_progress(comic_id, current_page, is_read)?;
        if let Some(item) = self
            .library
            .items
            .iter_mut()
            .find(|item| item.comic_id == comic_id)
        {
            item.is_read = is_read;
        }
        self.library.refresh_filter_cache();
        Ok(())
    }

    pub fn return_to_library(&mut self) {
        if let Some(reading) = &mut self.reading {
            reading.prefetch.cancel_all();
        }
        self.state = AppState::Library;
        self.reading = None;
    }

    pub fn purge_unavailable_from_view(&mut self) -> usize {
        let before = self.library.items.len();
        self.library
            .items
            .retain(|item| item.availability == ComicAvailability::Available);
        self.library.refresh_filter_cache();
        before - self.library.items.len()
    }

    pub fn resume_last_session(&mut self, service: &LibraryService) -> Result<bool, LibraryError> {
        let Some((comic, _progress)) = service.last_read_comic()? else {
            return Ok(false);
        };
        if comic.availability != ComicAvailability::Available || comic.page_count == 0 {
            return Ok(false);
        }
        self.open_comic(comic.id, comic.page_count as usize);
        self.hydrate_session_from_service(service, comic.id, comic.page_count);
        Ok(true)
    }

    pub fn active_progress_snapshot(&self) -> Option<ProgressSnapshot> {
        self.reading.as_ref().map(ReadingSession::progress_snapshot)
    }

    pub fn apply_reading_direction(&mut self, direction: ReadingDirection) {
        if let Some(reading) = &mut self.reading {
            reading.set_reading_direction(direction);
        }
    }
}

impl LibraryViewState {
    pub fn toggle_selection(&mut self, comic_id: i64) {
        if !self.selected_ids.insert(comic_id) {
            self.selected_ids.remove(&comic_id);
        }
    }

    pub fn is_selected(&self, comic_id: i64) -> bool {
        self.selected_ids.contains(&comic_id)
    }

    pub fn refresh_filter_cache(&mut self) {
        let existing_ids = self
            .items
            .iter()
            .map(|item| item.comic_id)
            .collect::<HashSet<_>>();
        self.selected_ids.retain(|id| existing_ids.contains(id));
        self.groups = self.build_groups();
        self.reconcile_active_filter();
        let query = self.search_query.to_lowercase();
        self.visible_indices = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if !query.is_empty() {
                    let title_match = item.title.to_lowercase().contains(&query);
                    let series_match = item.series.as_deref().is_some_and(|s| s.to_lowercase().contains(&query));
                    let writer_match = item.writer.as_deref().is_some_and(|w| w.to_lowercase().contains(&query));
                    if !title_match && !series_match && !writer_match {
                        return None;
                    }
                }
                let Some(active_filter) = &self.active_filter else {
                    return Some(index);
                };
                let matches_filter = match active_filter.kind {
                    LibraryGroupKind::Builtin => match active_filter.key.as_str() {
                        "continue_reading" => !item.is_read && item.current_page > 0,
                        "unread" => !item.is_read && item.current_page == 0,
                        "read" => item.is_read,
                        _ => false,
                    },
                    LibraryGroupKind::Series => {
                        item.series_key.as_deref() == Some(active_filter.key.as_str())
                    }
                    LibraryGroupKind::Folder => {
                        item.folder_key.as_deref() == Some(active_filter.key.as_str())
                    }
                };
                matches_filter.then_some(index)
            })
            .collect();
        // Natural ordering so "Issue 2" sorts before "Issue 10".
        match self.sort_option {
            LibrarySortOption::Title => self
                .visible_indices
                .sort_by(|&a, &b| natord::compare(&self.items[a].title, &self.items[b].title)),
            LibrarySortOption::DateAdded => self.visible_indices.sort_by(|&a, &b| self.items[b].comic_id.cmp(&self.items[a].comic_id)),
            LibrarySortOption::Series => self.visible_indices.sort_by(|&a, &b| {
                let s_a = self.items[a].series.as_deref().unwrap_or("");
                let s_b = self.items[b].series.as_deref().unwrap_or("");
                natord::compare(s_a, s_b)
                    .then_with(|| natord::compare(&self.items[a].title, &self.items[b].title))
            }),
            LibrarySortOption::Number => self.visible_indices.sort_by(|&a, &b| {
                let n_a = self.items[a].number.as_deref().unwrap_or("");
                let n_b = self.items[b].number.as_deref().unwrap_or("");
                // Comics without a number sort after numbered ones.
                match (n_a.is_empty(), n_b.is_empty()) {
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    _ => natord::compare(n_a, n_b)
                        .then_with(|| natord::compare(&self.items[a].title, &self.items[b].title)),
                }
            }),
        }
    }

    pub fn groups(&self) -> &[LibraryGroup] {
        &self.groups
    }

    pub fn visible_indices(&self) -> &[usize] {
        &self.visible_indices
    }

    pub fn visible_items(&self) -> impl Iterator<Item = &LibraryGridItem> {
        self.visible_indices
            .iter()
            .filter_map(|index| self.items.get(*index))
    }

    fn build_groups(&self) -> Vec<LibraryGroup> {
        let mut groups =
            std::collections::HashMap::<(LibraryGroupKind, String), LibraryGroup>::new();
        for item in &self.items {
            if let (Some(series_key), Some(series)) = (&item.series_key, &item.series) {
                let key = (LibraryGroupKind::Series, series_key.clone());
                groups
                    .entry(key)
                    .and_modify(|group| group.item_count += 1)
                    .or_insert_with(|| LibraryGroup {
                        kind: LibraryGroupKind::Series,
                        key: series_key.clone(),
                        label: series.clone(),
                        item_count: 1,
                    });
            }

            if let (Some(folder_key), Some(folder_label)) = (&item.folder_key, &item.folder_label) {
                let key = (LibraryGroupKind::Folder, folder_key.clone());
                groups
                    .entry(key)
                    .and_modify(|group| group.item_count += 1)
                    .or_insert_with(|| LibraryGroup {
                        kind: LibraryGroupKind::Folder,
                        key: folder_key.clone(),
                        label: folder_label.clone(),
                        item_count: 1,
                    });
            }
        }

        let mut groups = groups.into_values().collect::<Vec<_>>();
        for (key, label, count) in self.builtin_group_counts() {
            if count > 0 {
                groups.push(LibraryGroup {
                    kind: LibraryGroupKind::Builtin,
                    key: key.to_owned(),
                    label: label.to_owned(),
                    item_count: count,
                });
            }
        }
        groups.sort_by(compare_groups);
        groups
    }

    /// Counts for the three progress-derived shelves, in group order.
    fn builtin_group_counts(&self) -> [(&'static str, &'static str, usize); 3] {
        let mut continue_reading = 0;
        let mut unread = 0;
        let mut read = 0;
        for item in &self.items {
            if item.is_read {
                read += 1;
            } else if item.current_page > 0 {
                continue_reading += 1;
            } else {
                unread += 1;
            }
        }
        [
            ("continue_reading", "Continue Reading", continue_reading),
            ("unread", "Unread", unread),
            ("read", "Read", read),
        ]
    }

    /// Refreshes only the parts of the cached view that reading progress can
    /// change. Series and folder grouping, the search match set, and the sort
    /// order are all independent of progress, so the full rebuild is needed
    /// only when a progress-derived filter is active and the visible set can
    /// actually change. This keeps a page turn off the O(n log n) path.
    pub fn refresh_progress_derived_state(&mut self) {
        let builtin_filter_active = self
            .active_filter
            .as_ref()
            .is_some_and(|filter| filter.kind == LibraryGroupKind::Builtin);
        if builtin_filter_active {
            self.refresh_filter_cache();
            return;
        }
        self.refresh_builtin_group_counts();
    }

    fn refresh_builtin_group_counts(&mut self) {
        let mut membership_changed = false;
        for (key, label, count) in self.builtin_group_counts() {
            let existing = self
                .groups
                .iter()
                .position(|group| group.kind == LibraryGroupKind::Builtin && group.key == key);
            match (existing, count) {
                (Some(index), 0) => {
                    self.groups.remove(index);
                    membership_changed = true;
                }
                (Some(index), count) => self.groups[index].item_count = count,
                (None, 0) => {}
                (None, count) => {
                    self.groups.push(LibraryGroup {
                        kind: LibraryGroupKind::Builtin,
                        key: key.to_owned(),
                        label: label.to_owned(),
                        item_count: count,
                    });
                    membership_changed = true;
                }
            }
        }
        if membership_changed {
            self.groups.sort_by(compare_groups);
            self.reconcile_active_filter();
        }
    }

    pub fn reconcile_active_filter(&mut self) {
        let Some(active_filter) = &self.active_filter else {
            return;
        };
        let still_exists = self
            .groups
            .iter()
            .any(|group| group.kind == active_filter.kind && group.key == active_filter.key);
        if !still_exists {
            self.active_filter = None;
        }
    }
}

fn compare_groups(a: &LibraryGroup, b: &LibraryGroup) -> std::cmp::Ordering {
    a.kind
        .cmp(&b.kind)
        .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
        .then_with(|| a.key.cmp(&b.key))
}
