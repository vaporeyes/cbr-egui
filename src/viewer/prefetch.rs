use std::collections::HashSet;

use crate::decode::DecodeRequestId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageGeneration(pub u64);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrefetchState {
    pub page_count: usize,
    pub cached: HashSet<usize>,
    pub queued: HashSet<usize>,
    pub in_flight: HashSet<usize>,
}

pub fn prefetch_candidates(current_page: usize, state: &PrefetchState) -> Vec<usize> {
    [
        current_page.checked_add(1),
        current_page.checked_add(2),
        current_page.checked_sub(1),
    ]
    .into_iter()
    .flatten()
    .filter(|candidate| *candidate < state.page_count)
    .filter(|candidate| !state.cached.contains(candidate))
    .filter(|candidate| !state.queued.contains(candidate))
    .filter(|candidate| !state.in_flight.contains(candidate))
    .collect()
}

pub fn spread_prefetch_candidates(current_page: usize, state: &PrefetchState) -> Vec<usize> {
    prefetch_candidates(current_page, state)
}

pub fn result_matches_generation(
    active_generation: PageGeneration,
    result_generation: PageGeneration,
    active_request_id: Option<DecodeRequestId>,
    result_request_id: DecodeRequestId,
) -> bool {
    active_generation == result_generation
        && active_request_id.is_none_or(|active| active == result_request_id)
}

pub fn is_stale_result(
    active_generation: PageGeneration,
    result_generation: PageGeneration,
    active_request_id: Option<DecodeRequestId>,
    result_request_id: DecodeRequestId,
) -> bool {
    !result_matches_generation(
        active_generation,
        result_generation,
        active_request_id,
        result_request_id,
    )
}
