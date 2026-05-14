use serde::{Deserialize, Serialize};

use crate::viewer::layout::{PageId, Size2};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadingDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReadingLayoutMode {
    #[default]
    Paged,
    ContinuousVertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpreadGeneration(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageResourceIdentity {
    pub page_id: PageId,
    pub generation: SpreadGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpreadSideStatus {
    None,
    Pending { page_id: PageId },
    Ready { page_id: PageId },
    Failed { page_id: PageId, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpreadDecision {
    Single {
        page_id: PageId,
    },
    SinglePreStitched {
        page_id: PageId,
    },
    SingleNoNext {
        page_id: PageId,
    },
    Pair {
        left_page_id: PageId,
        right_page_id: PageId,
    },
    PairPending {
        left_page_id: PageId,
        right_page_id: PageId,
    },
    PairFailed {
        left_page_id: PageId,
        right_page_id: PageId,
        message: String,
    },
}

impl SpreadDecision {
    pub fn primary_page_id(&self) -> PageId {
        match *self {
            Self::Single { page_id }
            | Self::SinglePreStitched { page_id }
            | Self::SingleNoNext { page_id }
            | Self::Pair {
                left_page_id: page_id,
                ..
            }
            | Self::PairPending {
                left_page_id: page_id,
                ..
            }
            | Self::PairFailed {
                left_page_id: page_id,
                ..
            } => page_id,
        }
    }

    pub fn composition_key(&self) -> (PageId, Option<PageId>) {
        match *self {
            Self::Pair {
                left_page_id,
                right_page_id,
            }
            | Self::PairPending {
                left_page_id,
                right_page_id,
            }
            | Self::PairFailed {
                left_page_id,
                right_page_id,
                ..
            } => (left_page_id, Some(right_page_id)),
            _ => (self.primary_page_id(), None),
        }
    }
}

pub fn decide_spread(
    spread_mode_enabled: bool,
    current_page_id: PageId,
    current_size: Size2,
    next_page_id: Option<PageId>,
    next_status: SpreadSideStatus,
) -> SpreadDecision {
    if !spread_mode_enabled {
        return SpreadDecision::Single {
            page_id: current_page_id,
        };
    }

    if current_size.is_valid() && current_size.width > current_size.height {
        return SpreadDecision::SinglePreStitched {
            page_id: current_page_id,
        };
    }

    let Some(right_page_id) = next_page_id else {
        return SpreadDecision::SingleNoNext {
            page_id: current_page_id,
        };
    };

    match next_status {
        SpreadSideStatus::Ready { page_id } if page_id == right_page_id => SpreadDecision::Pair {
            left_page_id: current_page_id,
            right_page_id,
        },
        SpreadSideStatus::Failed { page_id, message } if page_id == right_page_id => {
            SpreadDecision::PairFailed {
                left_page_id: current_page_id,
                right_page_id,
                message,
            }
        }
        _ => SpreadDecision::PairPending {
            left_page_id: current_page_id,
            right_page_id,
        },
    }
}

pub fn spread_result_matches_generation(
    active_generation: SpreadGeneration,
    result_generation: SpreadGeneration,
    active_page_id: PageId,
    result_page_id: PageId,
) -> bool {
    active_generation == result_generation && active_page_id == result_page_id
}

pub fn ordered_spread_pages(
    left_page_id: PageId,
    right_page_id: PageId,
    direction: ReadingDirection,
) -> (PageId, PageId) {
    match direction {
        ReadingDirection::LeftToRight => (left_page_id, right_page_id),
        ReadingDirection::RightToLeft => (right_page_id, left_page_id),
    }
}

pub fn continuous_canvas_height(page_heights: impl IntoIterator<Item = f32>, gap: f32) -> f32 {
    let mut count = 0_usize;
    let total = page_heights
        .into_iter()
        .filter(|height| height.is_finite() && *height > 0.0)
        .inspect(|_| count += 1)
        .sum::<f32>();
    total + gap.max(0.0) * count.saturating_sub(1) as f32
}
