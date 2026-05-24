// ABOUTME: Decodes raw page image bytes into egui color images for display.
// ABOUTME: Defines cancellation-aware decode request and result payloads.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use eframe::egui;
use image::imageops::FilterType;

use super::error::DecodeError;

const MAX_DECODED_PIXELS: u64 = 100_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecodeRequestId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodePurpose {
    Direct,
    Prefetch,
}

#[derive(Debug, Clone)]
pub struct DecodeRequest {
    pub request_id: DecodeRequestId,
    pub page_index: usize,
    pub bytes: Vec<u8>,
    pub purpose: DecodePurpose,
    pub target_size: Option<[u32; 2]>,
    pub cancellation_token: Option<CancellationToken>,
}

#[derive(Debug, Clone)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeResult {
    pub request_id: DecodeRequestId,
    pub page_index: usize,
    pub purpose: DecodePurpose,
    pub outcome: Result<egui::ColorImage, DecodeError>,
}

pub fn decode_page(request: DecodeRequest) -> DecodeResult {
    let outcome = if request
        .cancellation_token
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
    {
        Err(DecodeError::Image("decode request cancelled".to_owned()))
    } else {
        decode_bytes(&request.bytes, request.target_size)
    };
    DecodeResult {
        request_id: request.request_id,
        page_index: request.page_index,
        purpose: request.purpose,
        outcome,
    }
}

fn decode_bytes(
    bytes: &[u8],
    target_size: Option<[u32; 2]>,
) -> Result<egui::ColorImage, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::EmptyBytes);
    }

    let mut image =
        image::load_from_memory(bytes).map_err(|err| DecodeError::Image(err.to_string()))?;
    if let Some([target_width, target_height]) = target_size
        && target_width > 0
        && target_height > 0
        && (image.width() > target_width || image.height() > target_height)
    {
        image = image.resize(target_width, target_height, FilterType::Lanczos3);
    }

    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_DECODED_PIXELS {
        return Err(DecodeError::ImageTooLarge { width, height });
    }

    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        rgba.as_raw(),
    ))
}
