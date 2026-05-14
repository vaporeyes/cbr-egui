use cbr_egui::cache::{
    DEFAULT_PAGE_CACHE_CAPACITY, MAX_PAGE_CACHE_CAPACITY, PageTextureCache, PageTextureCacheError,
};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn validates_default_and_max_capacity() {
    let cache = PageTextureCache::<String>::with_default_capacity();

    assert_eq!(cache.capacity(), DEFAULT_PAGE_CACHE_CAPACITY);
    assert!(PageTextureCache::<String>::new(MAX_PAGE_CACHE_CAPACITY).is_ok());
    assert!(matches!(
        PageTextureCache::<String>::new(MAX_PAGE_CACHE_CAPACITY + 1),
        Err(PageTextureCacheError::CapacityTooLarge { .. })
    ));
    assert!(matches!(
        PageTextureCache::<String>::new(0),
        Err(PageTextureCacheError::ZeroCapacity)
    ));
}

#[test]
fn inserts_and_reuses_cached_pages() {
    let mut cache = PageTextureCache::new(2).expect("cache");

    assert_eq!(cache.insert(1, "one"), None);
    assert!(cache.contains(1));
    assert_eq!(cache.get(1), Some(&"one"));
}

#[test]
fn exposes_cached_page_indices_for_prefetch_filtering() {
    let mut cache = PageTextureCache::new(3).expect("cache");

    cache.insert(1, "one");
    cache.insert(3, "three");

    let mut keys = cache.keys().collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(keys, [1, 3]);
}

#[test]
fn evicts_least_recently_used_page() {
    let mut cache = PageTextureCache::new(2).expect("cache");

    cache.insert(1, "one");
    cache.insert(2, "two");
    cache.insert(3, "three");

    assert!(!cache.contains(1));
    assert!(cache.contains(2));
    assert!(cache.contains(3));
}

#[test]
fn cache_access_refreshes_recency() {
    let mut cache = PageTextureCache::new(2).expect("cache");

    cache.insert(1, "one");
    cache.insert(2, "two");
    assert_eq!(cache.get(1), Some(&"one"));
    cache.insert(3, "three");

    assert!(cache.contains(1));
    assert!(!cache.contains(2));
    assert!(cache.contains(3));
}

#[test]
fn cache_never_exceeds_configured_capacity() {
    let mut cache = PageTextureCache::new(3).expect("cache");

    for page in 0..10 {
        cache.insert(page, page);
        assert!(cache.len() <= cache.capacity());
    }
}

#[test]
fn eviction_callback_releases_evicted_and_remaining_textures() {
    let released = Rc::new(RefCell::new(Vec::new()));
    {
        let released_for_cache = Rc::clone(&released);
        let mut cache = PageTextureCache::with_evictor(1, move |texture| {
            released_for_cache.borrow_mut().push(texture);
        })
        .expect("cache");

        assert_eq!(cache.insert(1, "one"), None);
        assert_eq!(cache.insert(2, "two"), None);
        assert_eq!(&*released.borrow(), &["one"]);
    }

    assert_eq!(&*released.borrow(), &["one", "two"]);
}
