use cbr_egui::viewer::{
    PageId, Point2, ReadingLayoutMode, Size2, ViewerState, ZoomAnchor, ZoomPanState,
    anchor_for_viewport, build_virtual_canvas, clamp_pan, pan_bounds, scroll_top_for_anchor,
};
use std::collections::HashMap;

fn assert_point_close(actual: Point2, expected: Point2) {
    assert!(
        (actual.x - expected.x).abs() < 0.01,
        "x: expected {}, got {}",
        expected.x,
        actual.x
    );
    assert!(
        (actual.y - expected.y).abs() < 0.01,
        "y: expected {}, got {}",
        expected.y,
        actual.y
    );
}

#[test]
fn scroll_zoom_increases_and_decreases_within_bounds() {
    let mut zoom_pan = ZoomPanState::default();

    zoom_pan.apply_scroll_zoom(120.0);
    assert!(zoom_pan.zoom > 1.0);

    let zoomed = zoom_pan.zoom;
    zoom_pan.apply_scroll_zoom(-120.0);
    assert!(zoom_pan.zoom < zoomed);
    assert!(zoom_pan.zoom >= zoom_pan.min_zoom);
}

#[test]
fn scroll_zoom_clamps_to_minimum_and_maximum() {
    let mut zoom_pan = ZoomPanState::default();

    for _ in 0..100 {
        zoom_pan.apply_scroll_zoom(240.0);
    }
    assert_eq!(zoom_pan.zoom, zoom_pan.max_zoom);

    for _ in 0..100 {
        zoom_pan.apply_scroll_zoom(-240.0);
    }
    assert_eq!(zoom_pan.zoom, zoom_pan.min_zoom);
    assert_eq!(zoom_pan.pan_offset, Point2::ZERO);
}

#[test]
fn anchored_zoom_preserves_pointer_focus() {
    let mut zoom_pan = ZoomPanState::default();

    zoom_pan.apply_zoom_factor(2.0, ZoomAnchor::from_viewport_delta(100.0, -50.0));

    assert_eq!(zoom_pan.zoom, 2.0);
    assert_point_close(zoom_pan.pan_offset, Point2::new(100.0, -50.0));
}

#[test]
fn keyboard_zoom_uses_center_anchor_and_reset_clears_pan() {
    let mut zoom_pan = ZoomPanState::default();
    zoom_pan.pan_offset = Point2::new(80.0, -20.0);

    zoom_pan.apply_zoom_factor(1.25, ZoomAnchor::CENTER);

    assert_eq!(zoom_pan.zoom, 1.25);
    assert_point_close(zoom_pan.pan_offset, Point2::new(100.0, -25.0));

    zoom_pan.reset_zoom();
    assert_eq!(zoom_pan.zoom, zoom_pan.min_zoom);
    assert_eq!(zoom_pan.pan_offset, Point2::ZERO);
}

#[test]
fn drag_pan_only_moves_when_page_exceeds_viewport() {
    let viewport = Size2::new(1000.0, 1000.0);
    let mut zoom_pan = ZoomPanState::default();

    zoom_pan.apply_drag_pan([200.0, 200.0], Size2::new(900.0, 900.0), viewport);
    assert_eq!(zoom_pan.pan_offset, Point2::ZERO);

    zoom_pan.apply_drag_pan([200.0, 100.0], Size2::new(1400.0, 1200.0), viewport);
    assert_point_close(zoom_pan.pan_offset, Point2::new(-200.0, -100.0));
}

#[test]
fn pan_clamps_to_useful_bounds() {
    let page = Size2::new(1800.0, 1400.0);
    let viewport = Size2::new(1000.0, 1000.0);

    assert_point_close(pan_bounds(page, viewport), Point2::new(400.0, 200.0));
    assert_point_close(
        clamp_pan(Point2::new(900.0, -900.0), page, viewport),
        Point2::new(400.0, -200.0),
    );
}

#[test]
fn repeated_zoom_and_pan_remains_stable() {
    let page = Size2::new(2200.0, 1800.0);
    let viewport = Size2::new(1000.0, 1000.0);
    let mut zoom_pan = ZoomPanState::default();

    for _ in 0..50 {
        zoom_pan.apply_scroll_zoom(80.0);
        let scaled_page = page.scaled(zoom_pan.zoom);
        zoom_pan.apply_drag_pan([75.0, -35.0], scaled_page, viewport);
    }

    let bounds = pan_bounds(page.scaled(zoom_pan.zoom), viewport);
    assert!(zoom_pan.zoom <= zoom_pan.max_zoom);
    assert!(zoom_pan.pan_offset.x.abs() <= bounds.x);
    assert!(zoom_pan.pan_offset.y.abs() <= bounds.y);
}

#[test]
fn page_identity_change_resets_zoom_and_pan() {
    let mut zoom_pan = ZoomPanState::default();
    zoom_pan.apply_scroll_zoom(500.0);
    zoom_pan.apply_drag_pan(
        [300.0, 300.0],
        Size2::new(2000.0, 2000.0),
        Size2::new(1000.0, 1000.0),
    );
    assert!(zoom_pan.zoom > zoom_pan.min_zoom);
    assert_ne!(zoom_pan.pan_offset, Point2::ZERO);

    zoom_pan.reset_for_page(PageId(1));
    assert_eq!(zoom_pan.zoom, zoom_pan.min_zoom);
    assert_eq!(zoom_pan.pan_offset, Point2::ZERO);
    assert_eq!(zoom_pan.reset_generation, 1);
}

#[test]
fn same_page_identity_preserves_zoom_and_pan() {
    let mut zoom_pan = ZoomPanState::default();

    zoom_pan.reset_for_page(PageId(1));
    zoom_pan.apply_scroll_zoom(500.0);
    zoom_pan.apply_drag_pan(
        [300.0, 300.0],
        Size2::new(2000.0, 2000.0),
        Size2::new(1000.0, 1000.0),
    );
    let preserved_zoom = zoom_pan.zoom;
    let preserved_pan = zoom_pan.pan_offset;
    let preserved_generation = zoom_pan.reset_generation;

    zoom_pan.reset_for_page(PageId(1));

    assert_eq!(zoom_pan.zoom, preserved_zoom);
    assert_eq!(zoom_pan.pan_offset, preserved_pan);
    assert_eq!(zoom_pan.reset_generation, preserved_generation);
}

#[test]
fn viewer_current_page_reset_ignores_stale_input_state() {
    let mut state: ViewerState<&str> = ViewerState::new();

    state.set_current_page(PageId(1));
    state.zoom_pan.apply_scroll_zoom(500.0);
    state.zoom_pan.pan_offset = Point2::new(250.0, -150.0);
    state.set_current_page(PageId(2));

    assert_eq!(state.current_page_id, Some(PageId(2)));
    assert_eq!(state.zoom_pan.zoom, state.zoom_pan.min_zoom);
    assert_eq!(state.zoom_pan.pan_offset, Point2::ZERO);
    assert_eq!(state.zoom_pan.reset_generation, 2);

    state.zoom_pan.apply_drag_pan(
        [1000.0, 1000.0],
        Size2::new(800.0, 800.0),
        Size2::new(1200.0, 1200.0),
    );
    assert_eq!(state.zoom_pan.pan_offset, Point2::ZERO);
}

#[test]
fn spread_composition_change_resets_zoom_and_pan() {
    let mut state: ViewerState<&str> = ViewerState::new();
    state.set_ready(PageId(1), "left", Size2::new(800.0, 1200.0));
    state.set_spread_mode_enabled(true);
    state.set_next_ready(PageId(2), "right", Size2::new(800.0, 1200.0));
    state.recompute_spread_decision(Some(PageId(2)));

    state.zoom_pan.apply_scroll_zoom(500.0);
    state.zoom_pan.pan_offset = Point2::new(100.0, 100.0);
    state.set_next_ready(PageId(3), "right", Size2::new(800.0, 1200.0));
    state.recompute_spread_decision(Some(PageId(3)));

    assert_eq!(state.zoom_pan.zoom, state.zoom_pan.min_zoom);
    assert_eq!(state.zoom_pan.pan_offset, Point2::ZERO);
}

#[test]
fn continuous_layout_mode_disables_spread_state() {
    let mut state: ViewerState<&str> = ViewerState::new();
    state.set_ready(PageId(1), "left", Size2::new(800.0, 1200.0));
    state.set_spread_mode_enabled(true);
    assert!(state.spread_mode_enabled);

    state.set_layout_mode(ReadingLayoutMode::ContinuousVertical);

    assert_eq!(state.layout_mode, ReadingLayoutMode::ContinuousVertical);
    assert!(!state.spread_mode_enabled);
    assert!(state.next_page_status.is_empty());
    assert!(state.spread_decision.is_none());
}

#[test]
fn scroll_anchor_restores_same_page_location_after_relayout() {
    let measurements = HashMap::new();
    let before = build_virtual_canvas(4, 200.0, &measurements, 0.5, 10.0);
    let anchor = anchor_for_viewport(&before, 450.0);
    let after = build_virtual_canvas(4, 300.0, &measurements, 0.5, 10.0);

    assert_eq!(anchor.page_index, 1);
    assert_eq!(scroll_top_for_anchor(&after, anchor), 650.0);
}
