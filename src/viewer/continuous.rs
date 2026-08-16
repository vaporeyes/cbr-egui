use std::collections::{HashMap, HashSet};

use crate::viewer::layout::Size2;
use crate::viewer::spread::continuous_canvas_height;

pub const CONTINUOUS_PAGE_GAP: f32 = 18.0;
pub const DEFAULT_PLACEHOLDER_ASPECT_RATIO: f32 = 2.0 / 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageMeasurementSource {
    Unknown,
    Placeholder,
    Actual,
    FailedPlaceholder,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageMeasurement {
    pub page_index: usize,
    pub size: Size2,
    pub source: PageMeasurementSource,
    pub failure: Option<String>,
}

impl PageMeasurement {
    pub fn placeholder(page_index: usize, size: Size2) -> Self {
        Self {
            page_index,
            size,
            source: PageMeasurementSource::Placeholder,
            failure: None,
        }
    }

    pub fn actual(page_index: usize, size: Size2) -> Self {
        Self {
            page_index,
            size,
            source: PageMeasurementSource::Actual,
            failure: None,
        }
    }

    pub fn failed_placeholder(page_index: usize, size: Size2, message: impl Into<String>) -> Self {
        Self {
            page_index,
            size,
            source: PageMeasurementSource::FailedPlaceholder,
            failure: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VirtualPageRect {
    pub page_index: usize,
    pub y: f32,
    pub size: Size2,
}

impl VirtualPageRect {
    pub fn bottom(self) -> f32 {
        self.y + self.size.height
    }

    pub fn intersects_y(self, top: f32, bottom: f32) -> bool {
        self.bottom() > top && self.y < bottom
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VirtualCanvas {
    pub total_height: f32,
    pub page_rects: Vec<VirtualPageRect>,
    pub viewport_width: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisiblePageWindow {
    pub visible_pages: Vec<usize>,
    pub overdraw_pages: Vec<usize>,
    pub viewport_top: f32,
    pub viewport_bottom: f32,
}

impl VisiblePageWindow {
    pub fn all_pages(&self) -> Vec<usize> {
        let mut pages = self
            .visible_pages
            .iter()
            .chain(self.overdraw_pages.iter())
            .copied()
            .collect::<Vec<_>>();
        pages.sort_unstable();
        pages.dedup();
        pages
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollAnchor {
    pub page_index: usize,
    pub intra_page_y: f32,
}

impl Default for ScrollAnchor {
    fn default() -> Self {
        Self {
            page_index: 0,
            intra_page_y: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContinuousScrollState {
    pub page_measurements: HashMap<usize, PageMeasurement>,
    pub scroll_anchor: ScrollAnchor,
    pub gap: f32,
    pub placeholder_ratio: f32,
    /// Monotonic counter bumped whenever measurements change, so layout
    /// consumers can skip rebuilding an unchanged virtual canvas.
    pub measurements_version: u64,
}

impl Default for ContinuousScrollState {
    fn default() -> Self {
        Self::new()
    }
}

impl ContinuousScrollState {
    pub fn new() -> Self {
        Self {
            page_measurements: HashMap::new(),
            scroll_anchor: ScrollAnchor::default(),
            gap: CONTINUOUS_PAGE_GAP,
            placeholder_ratio: DEFAULT_PLACEHOLDER_ASPECT_RATIO,
            measurements_version: 0,
        }
    }

    pub fn reset(&mut self) {
        // Keep the version monotonic across resets so cached layouts built
        // before the reset can never be mistaken for current.
        let version = self.measurements_version.saturating_add(1);
        *self = Self::new();
        self.measurements_version = version;
    }

    pub fn record_actual(&mut self, page_index: usize, size: Size2) {
        if !size.is_valid() {
            return;
        }
        self.placeholder_ratio = size.width / size.height;
        self.page_measurements
            .insert(page_index, PageMeasurement::actual(page_index, size));
        self.measurements_version = self.measurements_version.saturating_add(1);
    }

    pub fn record_failure(&mut self, page_index: usize, size: Size2, message: impl Into<String>) {
        let size = if size.is_valid() {
            size
        } else {
            placeholder_size(self.placeholder_ratio, 900.0)
        };
        self.page_measurements.insert(
            page_index,
            PageMeasurement::failed_placeholder(page_index, size, message),
        );
        self.measurements_version = self.measurements_version.saturating_add(1);
    }

    pub fn measured_pages(&self) -> HashSet<usize> {
        self.page_measurements
            .iter()
            .filter_map(|(page, measurement)| {
                (measurement.source == PageMeasurementSource::Actual).then_some(*page)
            })
            .collect()
    }
}

pub fn placeholder_size(aspect_ratio: f32, height: f32) -> Size2 {
    let ratio = if aspect_ratio.is_finite() && aspect_ratio > 0.0 {
        aspect_ratio
    } else {
        DEFAULT_PLACEHOLDER_ASPECT_RATIO
    };
    let height = if height.is_finite() && height > 0.0 {
        height
    } else {
        900.0
    };
    Size2::new(height * ratio, height)
}

pub fn page_measurement_for(
    page_index: usize,
    measurements: &HashMap<usize, PageMeasurement>,
    placeholder_ratio: f32,
    viewport_width: f32,
) -> PageMeasurement {
    if let Some(measurement) = measurements.get(&page_index) {
        return measurement.clone();
    }
    let width = viewport_width.max(1.0);
    let ratio = if placeholder_ratio.is_finite() && placeholder_ratio > 0.0 {
        placeholder_ratio
    } else {
        DEFAULT_PLACEHOLDER_ASPECT_RATIO
    };
    PageMeasurement::placeholder(page_index, Size2::new(width, width / ratio))
}

pub fn build_virtual_canvas(
    page_count: usize,
    viewport_width: f32,
    measurements: &HashMap<usize, PageMeasurement>,
    placeholder_ratio: f32,
    gap: f32,
) -> VirtualCanvas {
    let mut y = 0.0;
    let mut rects = Vec::with_capacity(page_count);
    for page_index in 0..page_count {
        let measurement =
            page_measurement_for(page_index, measurements, placeholder_ratio, viewport_width);
        let size = display_size_for_width(measurement.size, viewport_width);
        rects.push(VirtualPageRect {
            page_index,
            y,
            size,
        });
        y += size.height + gap.max(0.0);
    }
    let total_height = continuous_canvas_height(rects.iter().map(|rect| rect.size.height), gap);
    VirtualCanvas {
        total_height,
        page_rects: rects,
        viewport_width,
    }
}

pub fn display_size_for_width(size: Size2, viewport_width: f32) -> Size2 {
    if !size.is_valid() || !viewport_width.is_finite() || viewport_width <= 0.0 {
        return Size2::ZERO;
    }
    let scale = viewport_width / size.width;
    Size2::new(viewport_width, size.height * scale)
}

pub fn visible_page_window(
    canvas: &VirtualCanvas,
    viewport_top: f32,
    viewport_height: f32,
) -> VisiblePageWindow {
    let top = viewport_top.max(0.0);
    let bottom = (top + viewport_height.max(0.0)).min(canvas.total_height.max(0.0));
    let visible_pages = canvas
        .page_rects
        .iter()
        .filter(|rect| rect.intersects_y(top, bottom))
        .map(|rect| rect.page_index)
        .collect::<Vec<_>>();

    let first = visible_pages.first().copied();
    let last = visible_pages.last().copied();
    let mut overdraw_pages = Vec::new();
    if let Some(first) = first.and_then(|page| page.checked_sub(1)) {
        overdraw_pages.push(first);
    }
    if let Some(next) = last.and_then(|page| page.checked_add(1))
        && next < canvas.page_rects.len()
    {
        overdraw_pages.push(next);
    }

    VisiblePageWindow {
        visible_pages,
        overdraw_pages,
        viewport_top: top,
        viewport_bottom: bottom,
    }
}

pub fn anchor_for_viewport(canvas: &VirtualCanvas, viewport_top: f32) -> ScrollAnchor {
    let top = viewport_top.max(0.0);
    // Store position relative to content, not absolute document y, so pages
    // above the viewport can change height without moving the reader to a
    // different page.
    let Some(rect) = canvas
        .page_rects
        .iter()
        .find(|rect| rect.bottom() > top)
        .or_else(|| canvas.page_rects.last())
    else {
        return ScrollAnchor::default();
    };
    ScrollAnchor {
        page_index: rect.page_index,
        intra_page_y: (top - rect.y).max(0.0).min(rect.size.height),
    }
}

pub fn scroll_top_for_anchor(canvas: &VirtualCanvas, anchor: ScrollAnchor) -> f32 {
    canvas
        .page_rects
        .iter()
        .find(|rect| rect.page_index == anchor.page_index)
        .map(|rect| rect.y + anchor.intra_page_y.min(rect.size.height))
        .unwrap_or(0.0)
}
