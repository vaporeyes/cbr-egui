use crate::viewer::continuous::{VirtualCanvas, VirtualPageRect, VisiblePageWindow};
use crate::viewer::layout::{Point2, Size2, ViewMode, clamp_pan};
use crate::viewer::spread::{
    ReadingDirection, ReadingLayoutMode, SpreadDecision, SpreadSideStatus, decide_spread,
};

pub const DEFAULT_MIN_ZOOM: f32 = 0.25;
/// Zoom of 1.0 displays the page at its fitted size; resets return here
/// rather than to the floor, which now sits below fit.
pub const DEFAULT_FIT_ZOOM: f32 = 1.0;
pub const DEFAULT_MAX_ZOOM: f32 = 6.0;
pub const DEFAULT_SCROLL_ZOOM_SENSITIVITY: f32 = 0.0015;

pub use crate::viewer::layout::PageId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomPanState {
    pub zoom: f32,
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub pan_offset: Point2,
    pub reset_generation: u64,
    active_page_id: Option<PageId>,
    active_composition: Option<(PageId, Option<PageId>)>,
}

impl Default for ZoomPanState {
    fn default() -> Self {
        Self {
            zoom: DEFAULT_FIT_ZOOM,
            min_zoom: DEFAULT_MIN_ZOOM,
            max_zoom: DEFAULT_MAX_ZOOM,
            pan_offset: Point2::ZERO,
            reset_generation: 0,
            active_page_id: None,
            active_composition: None,
        }
    }
}

impl ZoomPanState {
    pub fn apply_scroll_zoom(&mut self, scroll_delta: f32) {
        self.apply_scroll_zoom_with_sensitivity(scroll_delta, DEFAULT_SCROLL_ZOOM_SENSITIVITY);
    }

    pub fn apply_scroll_zoom_with_sensitivity(&mut self, scroll_delta: f32, sensitivity: f32) {
        let sensitivity = if sensitivity.is_finite() && sensitivity > 0.0 {
            sensitivity
        } else {
            DEFAULT_SCROLL_ZOOM_SENSITIVITY
        };
        let multiplier = (scroll_delta * sensitivity).exp();
        self.apply_zoom_factor(multiplier, ZoomAnchor::CENTER);
    }

    pub fn apply_zoom_factor(&mut self, factor: f32, anchor: ZoomAnchor) {
        if !factor.is_finite() || factor <= 0.0 {
            self.zoom = self.zoom.clamp(self.min_zoom, self.max_zoom);
            return;
        }

        let old_zoom = self.zoom;
        self.zoom = (self.zoom * factor).clamp(self.min_zoom, self.max_zoom);
        if old_zoom <= 0.0 {
            return;
        }

        let actual_factor = self.zoom / old_zoom;
        self.pan_offset = Point2::new(
            self.pan_offset.x * actual_factor + anchor.viewport_delta.x * (actual_factor - 1.0),
            self.pan_offset.y * actual_factor + anchor.viewport_delta.y * (actual_factor - 1.0),
        );

        if self.zoom <= DEFAULT_FIT_ZOOM {
            self.pan_offset = Point2::ZERO;
        }
    }

    pub fn reset_zoom(&mut self) {
        self.zoom = DEFAULT_FIT_ZOOM;
        self.pan_offset = Point2::ZERO;
    }

    pub fn apply_drag_pan(&mut self, drag_delta: [f32; 2], page_size: Size2, viewport: Size2) {
        let requested = Point2::new(
            self.pan_offset.x - drag_delta[0],
            self.pan_offset.y - drag_delta[1],
        );
        self.pan_offset = clamp_pan(requested, page_size, viewport);
    }

    pub fn clamp_pan(&mut self, page_size: Size2, viewport: Size2) {
        self.pan_offset = clamp_pan(self.pan_offset, page_size, viewport);
    }

    pub fn reset_for_page(&mut self, page_id: PageId) {
        if self.active_page_id == Some(page_id) {
            return;
        }

        self.zoom = DEFAULT_FIT_ZOOM;
        self.pan_offset = Point2::ZERO;
        self.reset_generation = self.reset_generation.saturating_add(1);
        self.active_page_id = Some(page_id);
        self.active_composition = Some((page_id, None));
    }

    pub fn reset_for_composition(&mut self, left_page_id: PageId, right_page_id: Option<PageId>) {
        let composition = (left_page_id, right_page_id);
        if self.active_composition == Some(composition) {
            return;
        }

        self.zoom = DEFAULT_FIT_ZOOM;
        self.pan_offset = Point2::ZERO;
        self.reset_generation = self.reset_generation.saturating_add(1);
        self.active_page_id = Some(left_page_id);
        self.active_composition = Some(composition);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PageStatus<T> {
    Empty,
    Loading {
        page_id: PageId,
    },
    Ready {
        page_id: PageId,
        texture: T,
        pixel_size: Size2,
    },
    Failed {
        page_id: PageId,
        message: String,
    },
}

impl<T> PageStatus<T> {
    pub fn empty() -> Self {
        Self::Empty
    }

    pub fn loading(page_id: PageId) -> Self {
        Self::Loading { page_id }
    }

    pub fn ready(page_id: PageId, texture: T, pixel_size: Size2) -> Self {
        Self::Ready {
            page_id,
            texture,
            pixel_size,
        }
    }

    pub fn failed(page_id: PageId, message: impl Into<String>) -> Self {
        Self::Failed {
            page_id,
            message: message.into(),
        }
    }

    pub fn page_id(&self) -> Option<PageId> {
        match self {
            Self::Empty => None,
            Self::Loading { page_id }
            | Self::Ready { page_id, .. }
            | Self::Failed { page_id, .. } => Some(*page_id),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewerChrome {
    pub visible: bool,
    pub status_text: Option<String>,
    pub spread_toggle_visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageNavigationCommand {
    PreviousPage,
    NextPage,
    ScrollUp,
    ScrollDown,
    FirstPage,
    LastPage,
    GoToPage(usize),
}

/// Commands the viewer itself carries out while rendering. Every variant is
/// consumed by `apply_pending_view_command`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewCommand {
    Fit,
    Fill,
    FitWidth,
    FitHeight,
    OneToOne,
    ZoomIn,
    ZoomOut,
}

/// Commands the app layer carries out, because they need archive access, cache
/// invalidation, or a re-decode. These are kept in a separate slot from
/// `ViewCommand`: when they shared one, the viewer took them off the queue
/// while rendering and discarded them, so triggering any of these from the
/// keyboard did nothing in paged mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    ToggleSpread,
    ToggleContinuous,
    RotateLeft,
    RotateRight,
    ExtractPage,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContinuousPageStatus<T> {
    Loading,
    Ready { texture: T, pixel_size: Size2 },
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContinuousPage<T> {
    pub page_index: usize,
    pub rect: VirtualPageRect,
    pub status: ContinuousPageStatus<T>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomAnchor {
    pub viewport_delta: Point2,
}

impl ZoomAnchor {
    pub const CENTER: Self = Self {
        viewport_delta: Point2::ZERO,
    };

    pub const fn from_viewport_delta(x: f32, y: f32) -> Self {
        Self {
            viewport_delta: Point2::new(x, y),
        }
    }
}

pub fn corrupted_page_color_image(message: &str) -> eframe::egui::ColorImage {
    let width = 640;
    let height = 900;
    let mut pixels = vec![eframe::egui::Color32::from_rgb(36, 30, 30); width * height];
    for y in 0..height {
        for x in 0..width {
            if x < 8 || y < 8 || x >= width - 8 || y >= height - 8 {
                pixels[y * width + x] = eframe::egui::Color32::from_rgb(150, 55, 55);
            }
        }
    }
    let _ = message;
    eframe::egui::ColorImage {
        size: [width, height],
        pixels,
    }
}

impl Default for ViewerChrome {
    fn default() -> Self {
        Self {
            visible: true,
            status_text: None,
            spread_toggle_visible: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewerState<T> {
    /// Page the paged renderer is currently composing. This is the identity
    /// zoom, pan, and the spread decision are keyed on, and it is deliberately
    /// not the same thing as `ReadingSession::current_page_index`: in
    /// continuous mode the session tracks the page under the viewport while
    /// this stays put, because moving it would reset zoom and pan on every
    /// scroll tick.
    pub composed_page_id: Option<PageId>,
    pub view_mode: ViewMode,
    pub zoom_sensitivity: f32,
    pub zoom_pan: ZoomPanState,
    pub page_status: PageStatus<T>,
    pub viewport_size: Size2,
    pub chrome: ViewerChrome,
    pub spread_mode_enabled: bool,
    pub spread_decision: Option<SpreadDecision>,
    pub next_page_status: PageStatus<T>,
    pub reading_direction: ReadingDirection,
    pub layout_mode: ReadingLayoutMode,
    pub continuous_canvas: Option<VirtualCanvas>,
    pub continuous_visible_window: Option<VisiblePageWindow>,
    pub continuous_pages: Vec<ContinuousPage<T>>,
    pub continuous_pending_scroll_top: Option<f32>,
    pub pending_navigation: Option<PageNavigationCommand>,
    pub pending_view_command: Option<ViewCommand>,
    pub pending_app_command: Option<AppCommand>,
    pub pending_bookmark_toggle: bool,
    /// Accumulated plain-wheel scroll at fit zoom; turns the page once it
    /// crosses a threshold so trackpad micro-scrolls do not flip pages.
    pub wheel_page_accumulator: f32,
}

impl<T> Default for ViewerState<T> {
    fn default() -> Self {
        Self {
            composed_page_id: None,
            view_mode: ViewMode::Fit,
            zoom_sensitivity: DEFAULT_SCROLL_ZOOM_SENSITIVITY,
            zoom_pan: ZoomPanState::default(),
            page_status: PageStatus::Empty,
            viewport_size: Size2::ZERO,
            chrome: ViewerChrome::default(),
            spread_mode_enabled: false,
            spread_decision: None,
            next_page_status: PageStatus::Empty,
            reading_direction: ReadingDirection::LeftToRight,
            layout_mode: ReadingLayoutMode::Paged,
            continuous_canvas: None,
            continuous_visible_window: None,
            continuous_pages: Vec::new(),
            continuous_pending_scroll_top: None,
            pending_navigation: None,
            pending_view_command: None,
            pending_app_command: None,
            pending_bookmark_toggle: false,
            wheel_page_accumulator: 0.0,
        }
    }
}

impl<T> ViewerState<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_composed_page(&mut self, page_id: PageId) {
        if self.composed_page_id != Some(page_id) {
            self.composed_page_id = Some(page_id);
            self.zoom_pan.reset_for_page(page_id);
            self.spread_decision = None;
            self.next_page_status = PageStatus::Empty;
            self.wheel_page_accumulator = 0.0;
        }
    }

    pub fn set_spread_mode_enabled(&mut self, enabled: bool) {
        if enabled && self.layout_mode == ReadingLayoutMode::ContinuousVertical {
            return;
        }
        if self.spread_mode_enabled != enabled {
            self.spread_mode_enabled = enabled;
            if let Some(page_id) = self.composed_page_id {
                self.zoom_pan.reset_for_page(page_id);
            }
            self.spread_decision = None;
        }
    }

    pub fn set_reading_direction(&mut self, direction: ReadingDirection) {
        if self.reading_direction != direction {
            self.reading_direction = direction;
            if let Some(decision) = &self.spread_decision {
                let (left, right) = decision.composition_key();
                self.zoom_pan.reset_for_composition(left, right);
            }
        }
    }

    pub fn set_layout_mode(&mut self, layout_mode: ReadingLayoutMode) {
        if self.layout_mode != layout_mode {
            self.layout_mode = layout_mode;
            if layout_mode == ReadingLayoutMode::ContinuousVertical {
                self.spread_mode_enabled = false;
                self.next_page_status = PageStatus::Empty;
                self.spread_decision = None;
            }
            if let Some(page_id) = self.composed_page_id {
                self.zoom_pan.reset_for_page(page_id);
            }
        }
    }

    pub fn set_loading(&mut self, page_id: PageId) {
        self.set_composed_page(page_id);
        self.page_status = PageStatus::loading(page_id);
        self.chrome.status_text = Some("Loading page".to_owned());
    }

    pub fn set_ready(&mut self, page_id: PageId, texture: T, pixel_size: Size2) {
        self.set_composed_page(page_id);
        self.page_status = PageStatus::ready(page_id, texture, pixel_size);
        self.chrome.status_text = None;
    }

    pub fn set_next_loading(&mut self, page_id: PageId) {
        self.next_page_status = PageStatus::loading(page_id);
    }

    pub fn set_next_ready(&mut self, page_id: PageId, texture: T, pixel_size: Size2) {
        self.next_page_status = PageStatus::ready(page_id, texture, pixel_size);
    }

    pub fn set_next_failed(&mut self, page_id: PageId, message: impl Into<String>) {
        self.next_page_status = PageStatus::failed(page_id, message);
    }

    pub fn recompute_spread_decision(&mut self, next_page_id: Option<PageId>) {
        let PageStatus::Ready {
            page_id,
            pixel_size,
            ..
        } = &self.page_status
        else {
            self.spread_decision = None;
            return;
        };

        let next_status = match &self.next_page_status {
            PageStatus::Ready { page_id, .. } => SpreadSideStatus::Ready { page_id: *page_id },
            PageStatus::Loading { page_id } => SpreadSideStatus::Pending { page_id: *page_id },
            PageStatus::Failed { page_id, message } => SpreadSideStatus::Failed {
                page_id: *page_id,
                message: message.clone(),
            },
            PageStatus::Empty => SpreadSideStatus::None,
        };
        let decision = decide_spread(
            self.spread_mode_enabled,
            *page_id,
            *pixel_size,
            next_page_id,
            next_status,
        );
        let (left, right) = decision.composition_key();
        self.zoom_pan.reset_for_composition(left, right);
        self.spread_decision = Some(decision);
    }

    pub fn set_failed(&mut self, page_id: PageId, message: impl Into<String>) {
        let message = message.into();
        self.set_composed_page(page_id);
        self.page_status = PageStatus::failed(page_id, message.clone());
        self.chrome.status_text = Some(message);
    }
}
