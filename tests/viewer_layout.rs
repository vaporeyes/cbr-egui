use cbr_egui::viewer::{
    PageId, PageMeasurement, PageMeasurementSource, PageStatus, Size2, ViewMode, ViewerState,
    build_virtual_canvas, corrupted_page_color_image, page_display_size, page_measurement_for,
    placeholder_size, spread_page_sizes, visible_page_window,
};
use std::collections::HashMap;

fn assert_size_close(actual: Size2, expected: Size2) {
    assert!(
        (actual.width - expected.width).abs() < 0.01,
        "width: expected {}, got {}",
        expected.width,
        actual.width
    );
    assert!(
        (actual.height - expected.height).abs() < 0.01,
        "height: expected {}, got {}",
        expected.height,
        actual.height
    );
}

#[test]
fn fit_mode_handles_common_page_shapes() {
    let viewport = Size2::new(1000.0, 1000.0);

    assert_size_close(
        page_display_size(Size2::new(500.0, 1000.0), viewport, ViewMode::Fit),
        Size2::new(500.0, 1000.0),
    );
    assert_size_close(
        page_display_size(Size2::new(1000.0, 500.0), viewport, ViewMode::Fit),
        Size2::new(1000.0, 500.0),
    );
    assert_size_close(
        page_display_size(Size2::new(800.0, 800.0), viewport, ViewMode::Fit),
        Size2::new(1000.0, 1000.0),
    );
    assert_size_close(
        page_display_size(Size2::new(400.0, 1600.0), viewport, ViewMode::Fit),
        Size2::new(250.0, 1000.0),
    );
    assert_size_close(
        page_display_size(Size2::new(1600.0, 900.0), viewport, ViewMode::Fit),
        Size2::new(1000.0, 562.5),
    );
}

#[test]
fn fill_mode_preserves_aspect_ratio_and_fills_viewport() {
    let portrait = page_display_size(
        Size2::new(500.0, 1000.0),
        Size2::new(1000.0, 1000.0),
        ViewMode::Fill,
    );
    assert_size_close(portrait, Size2::new(1000.0, 2000.0));

    let landscape = page_display_size(
        Size2::new(1600.0, 900.0),
        Size2::new(1000.0, 1000.0),
        ViewMode::Fill,
    );
    assert_size_close(landscape, Size2::new(1777.7778, 1000.0));
}

#[test]
fn fit_width_scales_to_viewport_width_allowing_vertical_overflow() {
    let size = page_display_size(
        Size2::new(500.0, 1000.0),
        Size2::new(1000.0, 800.0),
        ViewMode::FitWidth,
    );
    assert_size_close(size, Size2::new(1000.0, 2000.0));
}

#[test]
fn fit_height_scales_to_viewport_height_allowing_horizontal_overflow() {
    let size = page_display_size(
        Size2::new(500.0, 1000.0),
        Size2::new(1000.0, 800.0),
        ViewMode::FitHeight,
    );
    assert_size_close(size, Size2::new(400.0, 800.0));
}

#[test]
fn invalid_dimensions_return_zero_size() {
    let viewport = Size2::new(1000.0, 1000.0);

    assert_eq!(
        page_display_size(Size2::new(0.0, 100.0), viewport, ViewMode::Fit),
        Size2::ZERO
    );
    assert_eq!(
        page_display_size(Size2::new(100.0, -1.0), viewport, ViewMode::Fit),
        Size2::ZERO
    );
    assert_eq!(
        page_display_size(
            Size2::new(100.0, 100.0),
            Size2::new(f32::NAN, 100.0),
            ViewMode::Fit
        ),
        Size2::ZERO
    );
}

#[test]
fn viewer_state_updates_page_statuses() {
    let mut state = ViewerState::new();
    let page = PageId(7);

    assert!(state.page_status.is_empty());

    state.set_loading(page);
    assert_eq!(state.current_page_id, Some(page));
    assert!(state.page_status.is_loading());
    assert_eq!(state.page_status.page_id(), Some(page));

    state.set_ready(page, "texture", Size2::new(800.0, 1200.0));
    assert!(state.page_status.is_ready());
    assert_eq!(
        state.page_status,
        PageStatus::ready(page, "texture", Size2::new(800.0, 1200.0))
    );

    state.set_failed(page, "decode failed");
    assert!(state.page_status.is_failed());
    assert_eq!(state.page_status, PageStatus::failed(page, "decode failed"));
}

#[test]
fn different_page_aspect_ratios_recompute_fit_size() {
    let viewport = Size2::new(1200.0, 800.0);
    let portrait = page_display_size(Size2::new(800.0, 1600.0), viewport, ViewMode::Fit);
    let landscape = page_display_size(Size2::new(1600.0, 800.0), viewport, ViewMode::Fit);

    assert_size_close(portrait, Size2::new(400.0, 800.0));
    assert_size_close(landscape, Size2::new(1200.0, 600.0));
}

#[test]
fn side_by_side_spread_layout_preserves_page_proportions() {
    let (left, right) = spread_page_sizes(
        Size2::new(800.0, 1200.0),
        Size2::new(800.0, 1200.0),
        Size2::new(1600.0, 900.0),
        ViewMode::Fit,
    );

    assert_size_close(left, Size2::new(600.0, 900.0));
    assert_size_close(right, Size2::new(600.0, 900.0));
}

#[test]
fn corrupted_page_fallback_image_is_generated() {
    let image = corrupted_page_color_image("bad bytes");

    assert_eq!(image.size, [640, 900]);
    assert_eq!(image.pixels.len(), 640 * 900);
}

#[test]
fn continuous_placeholder_uses_first_known_ratio_or_default() {
    let default = placeholder_size(f32::NAN, 900.0);
    assert_size_close(default, Size2::new(600.0, 900.0));

    let mut measurements = HashMap::new();
    measurements.insert(0, PageMeasurement::actual(0, Size2::new(1000.0, 2000.0)));
    let unknown = page_measurement_for(2, &measurements, 0.5, 300.0);

    assert_eq!(unknown.source, PageMeasurementSource::Placeholder);
    assert_size_close(unknown.size, Size2::new(300.0, 600.0));
}

#[test]
fn continuous_virtual_canvas_sums_ordered_page_rects() {
    let mut measurements = HashMap::new();
    measurements.insert(0, PageMeasurement::actual(0, Size2::new(100.0, 200.0)));
    measurements.insert(1, PageMeasurement::actual(1, Size2::new(100.0, 100.0)));
    let canvas = build_virtual_canvas(3, 200.0, &measurements, 0.5, 10.0);

    assert_eq!(canvas.page_rects.len(), 3);
    assert_size_close(canvas.page_rects[0].size, Size2::new(200.0, 400.0));
    assert_eq!(canvas.page_rects[0].y, 0.0);
    assert_eq!(canvas.page_rects[1].y, 410.0);
    assert_eq!(canvas.page_rects[2].y, 620.0);
    assert_size_close(canvas.page_rects[2].size, Size2::new(200.0, 400.0));
    assert_eq!(canvas.total_height, 1020.0);
}

#[test]
fn continuous_visible_window_includes_one_page_overdraw() {
    let measurements = HashMap::new();
    let canvas = build_virtual_canvas(5, 200.0, &measurements, 0.5, 10.0);
    let window = visible_page_window(&canvas, 430.0, 450.0);

    assert_eq!(window.visible_pages, [1, 2]);
    assert_eq!(window.overdraw_pages, [0, 3]);
    assert_eq!(window.all_pages(), [0, 1, 2, 3]);
}

#[test]
fn continuous_failed_placeholder_keeps_valid_size() {
    let failed = PageMeasurement::failed_placeholder(2, Size2::new(300.0, 450.0), "bad page");

    assert_eq!(failed.source, PageMeasurementSource::FailedPlaceholder);
    assert_eq!(failed.failure.as_deref(), Some("bad page"));
    assert_size_close(failed.size, Size2::new(300.0, 450.0));
}

#[test]
fn continuous_measurements_transition_from_placeholder_to_actual() {
    let mut measurements = HashMap::new();
    let placeholder = page_measurement_for(4, &measurements, 0.5, 300.0);
    assert_eq!(placeholder.source, PageMeasurementSource::Placeholder);

    measurements.insert(4, PageMeasurement::actual(4, Size2::new(200.0, 500.0)));
    let actual = page_measurement_for(4, &measurements, 0.5, 300.0);

    assert_eq!(actual.source, PageMeasurementSource::Actual);
    assert_size_close(actual.size, Size2::new(200.0, 500.0));
}

#[test]
fn continuous_resize_recomputes_positive_ordered_rectangles() {
    let measurements = HashMap::new();
    let narrow = build_virtual_canvas(4, 200.0, &measurements, 0.5, 10.0);
    let wide = build_virtual_canvas(4, 320.0, &measurements, 0.5, 10.0);

    assert!(wide.total_height > narrow.total_height);
    for pair in wide.page_rects.windows(2) {
        assert!(pair[0].size.is_valid());
        assert!(pair[1].size.is_valid());
        assert!(pair[0].bottom() <= pair[1].y);
    }
}
