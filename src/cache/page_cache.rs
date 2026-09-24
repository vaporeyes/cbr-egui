// ABOUTME: Bounds page resources by entry count and optional byte budgets.
// ABOUTME: Evicts least recently used resources and tracks cache revisions.
use std::num::NonZeroUsize;

use lru::LruCache;
use thiserror::Error;

pub const DEFAULT_PAGE_CACHE_CAPACITY: usize = 5;
/// Upper bound on cached page textures. Sized to cover the largest realistic
/// continuous-scroll working set (visible pages + overdraw on a tall display)
/// so on-screen pages are never evicted and re-decoded while still visible.
pub const MAX_PAGE_CACHE_CAPACITY: usize = 16;

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
    entries: LruCache<usize, (T, usize)>,
    evictor: Option<Box<dyn FnMut(T)>>,
    version: u64,
    bytes: usize,
    byte_budget: usize,
    weigh: fn(&T) -> usize,
    capacity_warning_emitted: bool,
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
            version: 0,
            bytes: 0,
            byte_budget: usize::MAX,
            weigh: std::mem::size_of_val,
            capacity_warning_emitted: false,
        })
    }

    pub fn with_byte_budget(
        capacity: usize,
        byte_budget: usize,
        weigh: fn(&T) -> usize,
    ) -> Result<Self, PageTextureCacheError> {
        let mut cache = Self::new(capacity)?;
        cache.byte_budget = byte_budget;
        cache.weigh = weigh;
        Ok(cache)
    }

    pub fn resident_bytes(&self) -> usize {
        self.bytes
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

    /// Grows the cache so it can hold at least `min_capacity` entries (clamped to
    /// `MAX_PAGE_CACHE_CAPACITY`). Used by continuous scroll to keep the entire
    /// on-screen working set resident, preventing evict/re-decode flicker. Never
    /// shrinks, so transient small windows do not discard still-useful textures.
    pub fn ensure_capacity(&mut self, min_capacity: usize) {
        if min_capacity > MAX_PAGE_CACHE_CAPACITY && !self.capacity_warning_emitted {
            // A working set beyond the cap reintroduces the evict/re-decode
            // flicker this cache is sized to prevent; surface it once.
            eprintln!(
                "page texture cache: working set of {min_capacity} exceeds cap {MAX_PAGE_CACHE_CAPACITY}; expect re-decodes"
            );
            self.capacity_warning_emitted = true;
        }
        let target = min_capacity.clamp(1, MAX_PAGE_CACHE_CAPACITY);
        if target > self.entries.cap().get() {
            self.entries
                .resize(NonZeroUsize::new(target).expect("target clamped to >= 1"));
        }
    }

    pub fn get(&mut self, page_index: usize) -> Option<&T> {
        self.entries.get(&page_index).map(|(texture, _)| texture)
    }

    pub fn insert(&mut self, page_index: usize, texture: T) -> Option<T> {
        let weight = (self.weigh)(&texture);
        if weight > self.byte_budget {
            return Some(texture);
        }
        self.version = self.version.saturating_add(1);
        let mut evicted = self
            .entries
            .push(page_index, (texture, weight))
            .map(|(_, entry)| entry);
        if let Some((_, weight)) = &evicted {
            self.bytes -= weight;
        }
        self.bytes += weight;
        if let Some(evictor) = &mut self.evictor
            && let Some((value, _)) = evicted.take()
        {
            evictor(value);
        }
        while self.bytes > self.byte_budget {
            if let Some((_, (value, weight))) = self.entries.pop_lru() {
                self.bytes -= weight;
                if let Some(evictor) = &mut self.evictor {
                    evictor(value);
                } else {
                    evicted = Some((value, weight));
                }
            }
        }
        evicted.map(|(value, _)| value)
    }

    pub fn contains(&self, page_index: usize) -> bool {
        self.entries.contains(&page_index)
    }

    /// Monotonic counter bumped on every insert or clear, so consumers can
    /// detect content changes without comparing entries.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn clear(&mut self) {
        self.bytes = 0;
        self.version = self.version.saturating_add(1);
        if let Some(evictor) = &mut self.evictor {
            while let Some((_key, (value, _))) = self.entries.pop_lru() {
                evictor(value);
            }
        } else {
            self.entries.clear();
        }
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
            while let Some((_key, (value, _))) = self.entries.pop_lru() {
                evictor(value);
            }
        }
    }
}
