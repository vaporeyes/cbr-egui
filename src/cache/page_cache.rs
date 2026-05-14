use std::num::NonZeroUsize;

use lru::LruCache;
use thiserror::Error;

pub const DEFAULT_PAGE_CACHE_CAPACITY: usize = 5;
pub const MAX_PAGE_CACHE_CAPACITY: usize = 10;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PageTextureCacheError {
    #[error("cache capacity must be at least one")]
    ZeroCapacity,
    #[error("cache capacity {capacity} exceeds max {max}")]
    CapacityTooLarge { capacity: usize, max: usize },
}

/// Bounded LRU cache for main-thread display resources such as
/// `egui::TextureHandle`. Background decode workers must not create or insert
/// texture handles; callers insert them only after main-thread texture upload.
pub struct PageTextureCache<T> {
    entries: LruCache<usize, T>,
    evictor: Option<Box<dyn FnMut(T)>>,
}

impl<T> PageTextureCache<T> {
    pub fn new(capacity: usize) -> Result<Self, PageTextureCacheError> {
        if capacity == 0 {
            return Err(PageTextureCacheError::ZeroCapacity);
        }
        if capacity > MAX_PAGE_CACHE_CAPACITY {
            return Err(PageTextureCacheError::CapacityTooLarge {
                capacity,
                max: MAX_PAGE_CACHE_CAPACITY,
            });
        }

        Ok(Self {
            entries: LruCache::new(NonZeroUsize::new(capacity).expect("capacity checked")),
            evictor: None,
        })
    }

    pub fn with_evictor(
        capacity: usize,
        evictor: impl FnMut(T) + 'static,
    ) -> Result<Self, PageTextureCacheError> {
        let mut cache = Self::new(capacity)?;
        cache.evictor = Some(Box::new(evictor));
        Ok(cache)
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_PAGE_CACHE_CAPACITY).expect("default cache capacity is valid")
    }

    pub fn get(&mut self, page_index: usize) -> Option<&T> {
        self.entries.get(&page_index)
    }

    pub fn insert(&mut self, page_index: usize, texture: T) -> Option<T> {
        let evicted = self
            .entries
            .push(page_index, texture)
            .map(|(_, value)| value);
        match (&mut self.evictor, evicted) {
            (Some(evictor), Some(value)) => {
                evictor(value);
                None
            }
            (_, evicted) => evicted,
        }
    }

    pub fn contains(&self, page_index: usize) -> bool {
        self.entries.contains(&page_index)
    }

    pub fn keys(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries.iter().map(|(key, _)| *key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.entries.cap().get()
    }
}

impl<T> Drop for PageTextureCache<T> {
    fn drop(&mut self) {
        if let Some(evictor) = &mut self.evictor {
            while let Some((_key, value)) = self.entries.pop_lru() {
                evictor(value);
            }
        }
    }
}
