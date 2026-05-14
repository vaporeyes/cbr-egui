#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size2 {
    pub width: f32,
    pub height: f32,
}

impl Size2 {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn is_valid(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width > 0.0 && self.height > 0.0
    }

    pub fn scaled(self, scale: f32) -> Self {
        if !self.is_valid() || !scale.is_finite() || scale <= 0.0 {
            return Self::ZERO;
        }

        Self {
            width: self.width * scale,
            height: self.height * scale,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point2 {
    pub x: f32,
    pub y: f32,
}

impl Point2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ViewMode {
    #[default]
    Fit,
    Fill,
}

pub fn page_display_size(page: Size2, viewport: Size2, mode: ViewMode) -> Size2 {
    if !page.is_valid() || !viewport.is_valid() {
        return Size2::ZERO;
    }

    let width_scale = viewport.width / page.width;
    let height_scale = viewport.height / page.height;
    let scale = match mode {
        ViewMode::Fit => width_scale.min(height_scale),
        ViewMode::Fill => width_scale.max(height_scale),
    };

    page.scaled(scale)
}

pub fn spread_display_size(left: Size2, right: Size2, viewport: Size2, mode: ViewMode) -> Size2 {
    if !left.is_valid() {
        return Size2::ZERO;
    }
    if !right.is_valid() {
        return page_display_size(left, viewport, mode);
    }

    page_display_size(
        Size2::new(left.width + right.width, left.height.max(right.height)),
        viewport,
        mode,
    )
}

pub fn spread_page_sizes(
    left: Size2,
    right: Size2,
    viewport: Size2,
    mode: ViewMode,
) -> (Size2, Size2) {
    let combined = spread_display_size(left, right, viewport, mode);
    if !combined.is_valid() || !left.is_valid() || !right.is_valid() {
        return (combined, Size2::ZERO);
    }

    let original_width = left.width + right.width;
    let scale = combined.width / original_width;
    (left.scaled(scale), right.scaled(scale))
}

pub fn pan_bounds(page_size: Size2, viewport: Size2) -> Point2 {
    if !page_size.is_valid() || !viewport.is_valid() {
        return Point2::ZERO;
    }

    Point2 {
        x: ((page_size.width - viewport.width) / 2.0).max(0.0),
        y: ((page_size.height - viewport.height) / 2.0).max(0.0),
    }
}

pub fn clamp_pan(pan: Point2, page_size: Size2, viewport: Size2) -> Point2 {
    let bounds = pan_bounds(page_size, viewport);

    Point2 {
        x: clamp_axis(pan.x, bounds.x),
        y: clamp_axis(pan.y, bounds.y),
    }
}

fn clamp_axis(value: f32, limit: f32) -> f32 {
    if !value.is_finite() || limit == 0.0 {
        0.0
    } else {
        value.clamp(-limit, limit)
    }
}
