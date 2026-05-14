use std::collections::{HashMap, HashSet};

use crate::cache::{PageTextureCache, PageTextureCacheError};
use crate::decode::{CancellationToken, DecodeRequestId, WorkerPool};
use crate::library::{ComicAvailability, LibraryError, LibraryGridItem, LibraryService};
use crate::viewer::{ContinuousScrollState, PageGeneration, PageId, PrefetchState, ViewerState};

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
}

pub struct CachedPage<T> {
    pub texture: T,
    pub pixel_size: crate::viewer::Size2,
}

#[derive(Debug, Clone)]
pub struct InFlightPrefetch {
    pub page_index: usize,
    pub request_id: DecodeRequestId,
    pub generation: PageGeneration,
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
        cancellation_token: CancellationToken,
    ) {
        self.queued_pages.remove(&page_index);
        self.in_flight.insert(
            page_index,
            InFlightPrefetch {
                page_index,
                request_id,
                generation: self.generation,
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
        }
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

    pub fn set_spread_mode_enabled(&mut self, enabled: bool) {
        self.spread_mode_enabled = enabled;
        self.viewer_state.set_spread_mode_enabled(enabled);
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
        if let Some(reading) = &mut self.reading {
            reading.prefetch.cancel_all();
        }
        self.state = AppState::Reading(comic_id);
        self.library.selected_comic_id = Some(comic_id);
        self.reading = Some(ReadingSession::new(comic_id, page_count));
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
        before - self.library.items.len()
    }

    pub fn resume_last_session(&mut self, service: &LibraryService) -> Result<bool, LibraryError> {
        let Some((comic, progress)) = service.last_read_comic()? else {
            return Ok(false);
        };
        self.open_comic(comic.id, comic.page_count as usize);
        if let Some(reading) = &mut self.reading {
            reading.set_current_page(progress.current_page as usize);
        }
        Ok(true)
    }
}
