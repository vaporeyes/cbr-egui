use cbr_egui::cache::{
    DEFAULT_PAGE_CACHE_CAPACITY, MAX_PAGE_CACHE_CAPACITY, PageTextureCache, PageTextureCacheError,
};
use cbr_egui::viewer::{build_virtual_canvas, visible_page_window};
use std::cell::RefCell;
use std::collections::HashMap;
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

// Reproduces the continuous-scroll flicker: a tall viewport over short
// (landscape) pages produces an on-screen working set larger than the default
// texture cache, so visible pages get evicted and would be re-decoded each
// frame. Sizing the cache to the working set keeps every visible page resident.
#[test]
fn continuous_working_set_exceeds_default_capacity() {
    let canvas = build_virtual_canvas(20, 800.0, &HashMap::new(), 1.6, 18.0);
    let window = visible_page_window(&canvas, 0.0, 3000.0);
    let working_set = window.all_pages();

    // Precondition: the visible+overdraw window is larger than the default cache.
    assert!(working_set.len() > DEFAULT_PAGE_CACHE_CAPACITY);

    // With the default capacity, decoding the whole working set evicts some pages
    // that are still on screen (the flicker source).
    let mut cache = PageTextureCache::<u8>::with_default_capacity();
    for &page in &working_set {
        cache.insert(page, 0);
    }
    let resident = working_set.iter().filter(|p| cache.contains(**p)).count();
    assert!(resident < working_set.len());
}

#[test]
fn ensure_capacity_keeps_continuous_working_set_resident() {
    let canvas = build_virtual_canvas(20, 800.0, &HashMap::new(), 1.6, 18.0);
    let window = visible_page_window(&canvas, 0.0, 3000.0);
    let working_set = window.all_pages();

    let mut cache = PageTextureCache::<u8>::with_default_capacity();
    cache.ensure_capacity(working_set.len());
    for &page in &working_set {
        cache.insert(page, 0);
    }

    // Every visible page stays cached, so none flicker back to Loading.
    assert!(working_set.iter().all(|p| cache.contains(*p)));
}

#[test]
fn ensure_capacity_grows_but_never_shrinks_and_is_clamped() {
    let mut cache = PageTextureCache::<u8>::with_default_capacity();
    assert_eq!(cache.capacity(), DEFAULT_PAGE_CACHE_CAPACITY);

    cache.ensure_capacity(8);
    assert_eq!(cache.capacity(), 8);

    // Smaller request does not shrink.
    cache.ensure_capacity(3);
    assert_eq!(cache.capacity(), 8);

    // Clamped to the maximum.
    cache.ensure_capacity(MAX_PAGE_CACHE_CAPACITY + 5);
    assert_eq!(cache.capacity(), MAX_PAGE_CACHE_CAPACITY);
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
