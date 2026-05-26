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
    LibraryGroup, LibraryGroupKind, LibraryService,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryViewState {
    pub items: Vec<LibraryGridItem>,
    pub selected_comic_id: Option<i64>,
    pub status_text: Option<String>,
    pub view_mode: LibraryViewMode,
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
    pub archive_cache: ReadingArchiveCache,
    pub goto_input: String,
    pub bookmarks: BTreeSet<usize>,
    pub bookmarks_loaded: bool,
    pub rotation: Rotation,
    pub adjustments: ImageAdjustments,
    pub show_adjustments: bool,
    pub show_page_sidebar: bool,
    pub page_thumbnails: HashMap<usize, T>,
    pub pending_page_thumbnails: HashSet<usize>,
    pub failed_page_thumbnails: HashSet<usize>,
    pub page_thumbnail_pool: Option<WorkerPool>,
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
            archive_cache: ReadingArchiveCache::default(),
            goto_input: String::new(),
            bookmarks: BTreeSet::new(),
            bookmarks_loaded: false,
            rotation: Rotation::None,
            adjustments: ImageAdjustments::default(),
            show_adjustments: false,
            show_page_sidebar: true,
            page_thumbnails: HashMap::new(),
            pending_page_thumbnails: HashSet::new(),
            failed_page_thumbnails: HashSet::new(),
            page_thumbnail_pool: WorkerPool::start(1, 64).ok(),
        }
    }

    pub fn invalidate_page_thumbnails(&mut self) {
        self.page_thumbnails.clear();
        self.pending_page_thumbnails.clear();
        self.failed_page_thumbnails.clear();
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
            archive_cache: ReadingArchiveCache::default(),
            goto_input: String::new(),
            bookmarks: BTreeSet::new(),
            bookmarks_loaded: false,
            rotation: Rotation::None,
            adjustments: ImageAdjustments::default(),
            show_adjustments: false,
            show_page_sidebar: true,
            page_thumbnails: HashMap::new(),
            pending_page_thumbnails: HashSet::new(),
            failed_page_thumbnails: HashSet::new(),
            page_thumbnail_pool: WorkerPool::start(1, 64).ok(),
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
            is_read: self.current_page_index + 1 >= self.page_count,
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

        self.open_comic(item.comic_id, item.page_count as usize);
        true
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
        let Some((comic, progress)) = service.last_read_comic()? else {
            return Ok(false);
        };
        if comic.availability != ComicAvailability::Available || comic.page_count == 0 {
            return Ok(false);
        }
        self.open_comic(comic.id, comic.page_count as usize);
        if let Some(reading) = &mut self.reading {
            reading.set_current_page(progress.current_page as usize);
        }
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
        self.visible_indices = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let Some(active_filter) = &self.active_filter else {
                    return Some(index);
                };
                let matches_filter = match active_filter.kind {
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
        groups.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
                .then_with(|| a.key.cmp(&b.key))
        });
        groups
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
