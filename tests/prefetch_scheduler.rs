use std::collections::HashSet;

use cbr_egui::decode::DecodeRequestId;
use cbr_egui::viewer::{
    PageGeneration, PrefetchState, is_stale_result, prefetch_candidates, result_matches_generation,
};

#[test]
fn middle_page_prefetches_next_following_previous() {
    let state = PrefetchState {
        page_count: 10,
        ..PrefetchState::default()
    };

    assert_eq!(prefetch_candidates(4, &state), [5, 6, 3]);
}

#[test]
fn first_and_last_pages_skip_out_of_range_candidates() {
    let state = PrefetchState {
        page_count: 3,
        ..PrefetchState::default()
    };

    assert_eq!(prefetch_candidates(0, &state), [1, 2]);
    assert_eq!(prefetch_candidates(2, &state), [1]);
}

#[test]
fn cached_pages_are_not_prefetched_again() {
    let state = PrefetchState {
        page_count: 10,
        cached: HashSet::from([5]),
        ..PrefetchState::default()
    };

    assert_eq!(prefetch_candidates(4, &state), [6, 3]);
}

#[test]
fn queued_and_in_flight_pages_are_not_prefetched_again() {
    let state = PrefetchState {
        page_count: 10,
        queued: HashSet::from([5]),
        in_flight: HashSet::from([6]),
        ..PrefetchState::default()
    };

    assert_eq!(prefetch_candidates(4, &state), [3]);
}

#[test]
fn all_candidate_states_are_excluded_from_dispatch() {
    let state = PrefetchState {
        page_count: 10,
        cached: HashSet::from([5]),
        queued: HashSet::from([6]),
        in_flight: HashSet::from([3]),
    };

    assert!(prefetch_candidates(4, &state).is_empty());
}

#[test]
fn stale_result_helper_detects_generation_or_request_mismatch() {
    assert!(result_matches_generation(
        PageGeneration(2),
        PageGeneration(2),
        Some(DecodeRequestId(10)),
        DecodeRequestId(10),
    ));
    assert!(is_stale_result(
        PageGeneration(2),
        PageGeneration(1),
        Some(DecodeRequestId(10)),
        DecodeRequestId(10),
    ));
    assert!(is_stale_result(
        PageGeneration(2),
        PageGeneration(2),
        Some(DecodeRequestId(10)),
        DecodeRequestId(11),
    ));
}
