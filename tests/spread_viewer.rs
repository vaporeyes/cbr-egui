use cbr_egui::viewer::{
    PageId, ReadingDirection, Size2, SpreadDecision, SpreadGeneration, SpreadSideStatus,
    ViewerState, continuous_canvas_height, decide_spread, ordered_spread_pages,
    spread_result_matches_generation,
};

#[test]
fn spread_disabled_uses_single_page() {
    let decision = decide_spread(
        false,
        PageId(1),
        Size2::new(800.0, 1200.0),
        Some(PageId(2)),
        SpreadSideStatus::Ready { page_id: PageId(2) },
    );

    assert_eq!(decision, SpreadDecision::Single { page_id: PageId(1) });
}

#[test]
fn landscape_page_is_prestitched_spread() {
    let decision = decide_spread(
        true,
        PageId(3),
        Size2::new(1800.0, 900.0),
        Some(PageId(4)),
        SpreadSideStatus::Ready { page_id: PageId(4) },
    );

    assert_eq!(
        decision,
        SpreadDecision::SinglePreStitched { page_id: PageId(3) }
    );
}

#[test]
fn portrait_and_square_pages_pair_with_ready_next_page() {
    for size in [Size2::new(800.0, 1200.0), Size2::new(1000.0, 1000.0)] {
        let decision = decide_spread(
            true,
            PageId(1),
            size,
            Some(PageId(2)),
            SpreadSideStatus::Ready { page_id: PageId(2) },
        );

        assert_eq!(
            decision,
            SpreadDecision::Pair {
                left_page_id: PageId(1),
                right_page_id: PageId(2)
            }
        );
    }
}

#[test]
fn last_page_and_missing_next_page_are_recoverable() {
    assert_eq!(
        decide_spread(
            true,
            PageId(9),
            Size2::new(800.0, 1200.0),
            None,
            SpreadSideStatus::None,
        ),
        SpreadDecision::SingleNoNext { page_id: PageId(9) }
    );

    assert_eq!(
        decide_spread(
            true,
            PageId(1),
            Size2::new(800.0, 1200.0),
            Some(PageId(2)),
            SpreadSideStatus::Failed {
                page_id: PageId(2),
                message: "missing".to_owned()
            },
        ),
        SpreadDecision::PairFailed {
            left_page_id: PageId(1),
            right_page_id: PageId(2),
            message: "missing".to_owned()
        }
    );
}

#[test]
fn stale_next_page_result_is_ignored_by_generation() {
    assert!(spread_result_matches_generation(
        SpreadGeneration(3),
        SpreadGeneration(3),
        PageId(2),
        PageId(2),
    ));
    assert!(!spread_result_matches_generation(
        SpreadGeneration(3),
        SpreadGeneration(2),
        PageId(2),
        PageId(2),
    ));
    assert!(!spread_result_matches_generation(
        SpreadGeneration(3),
        SpreadGeneration(3),
        PageId(2),
        PageId(4),
    ));
}

#[test]
fn rtl_reading_direction_swaps_spread_page_order() {
    assert_eq!(
        ordered_spread_pages(PageId(1), PageId(2), ReadingDirection::LeftToRight),
        (PageId(1), PageId(2))
    );
    assert_eq!(
        ordered_spread_pages(PageId(1), PageId(2), ReadingDirection::RightToLeft),
        (PageId(2), PageId(1))
    );
}

#[test]
fn viewer_reading_direction_preference_updates_spread_order() {
    let mut state: ViewerState<&str> = ViewerState::new();
    state.set_ready(PageId(1), "left", Size2::new(800.0, 1200.0));
    state.set_next_ready(PageId(2), "right", Size2::new(800.0, 1200.0));
    state.set_spread_mode_enabled(true);
    state.set_reading_direction(ReadingDirection::RightToLeft);
    state.recompute_spread_decision(Some(PageId(2)));

    assert_eq!(
        ordered_spread_pages(PageId(1), PageId(2), state.reading_direction),
        (PageId(2), PageId(1))
    );
}

#[test]
fn continuous_canvas_height_sums_visible_pages_and_gaps() {
    assert_eq!(continuous_canvas_height([100.0, 200.0, 300.0], 12.0), 624.0);
    assert_eq!(
        continuous_canvas_height([0.0, f32::NAN, 200.0], 12.0),
        200.0
    );
}
