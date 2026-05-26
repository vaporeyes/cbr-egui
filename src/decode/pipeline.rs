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
    Thumbnail,
}

/// Clockwise page rotation applied during decode, so cached textures and layout
/// math always operate on the already-rotated pixel dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    None,
    Cw90,
    Cw180,
    Cw270,
}

impl Rotation {
    pub fn rotate_right(self) -> Self {
        match self {
            Self::None => Self::Cw90,
            Self::Cw90 => Self::Cw180,
            Self::Cw180 => Self::Cw270,
            Self::Cw270 => Self::None,
        }
    }

    pub fn rotate_left(self) -> Self {
        match self {
            Self::None => Self::Cw270,
            Self::Cw270 => Self::Cw180,
            Self::Cw180 => Self::Cw90,
            Self::Cw90 => Self::None,
        }
    }

    /// True for quarter turns, which swap a page's width and height.
    pub fn swaps_dimensions(self) -> bool {
        matches!(self, Self::Cw90 | Self::Cw270)
    }
}

/// Per-channel brightness/contrast/gamma applied during decode. Identity values
/// (the default) skip all per-pixel work.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageAdjustments {
    /// -1.0 ..= 1.0, 0.0 is unchanged.
    pub brightness: f32,
    /// -1.0 ..= 1.0, 0.0 is unchanged.
    pub contrast: f32,
    /// 0.1 ..= 3.0, 1.0 is unchanged.
    pub gamma: f32,
    /// 0.0 ..= 1.0 mix toward luminance, 0.0 is unchanged.
    pub grayscale: f32,
    /// Posterize luma into N bands for value study. Always forces grayscale and
    /// overrides the grayscale mix while active.
    pub value_study: bool,
    /// Number of value bands when `value_study` is on. Range 2..=8.
    pub value_bands: u32,
}

pub const VALUE_BANDS_MIN: u32 = 2;
pub const VALUE_BANDS_MAX: u32 = 8;
pub const VALUE_BANDS_DEFAULT: u32 = 4;

impl Default for ImageAdjustments {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            gamma: 1.0,
            grayscale: 0.0,
            value_study: false,
            value_bands: VALUE_BANDS_DEFAULT,
        }
    }
}

impl ImageAdjustments {
    pub fn is_identity(&self) -> bool {
        self.brightness == 0.0
            && self.contrast == 0.0
            && self.gamma == 1.0
            && self.grayscale == 0.0
            && !self.value_study
    }

    fn has_per_channel_adjustment(&self) -> bool {
        self.brightness != 0.0 || self.contrast != 0.0 || self.gamma != 1.0
    }

    /// Maps a single 0..=255 channel value through contrast, brightness, then
    /// gamma. Used to build the decode lookup table and unit-tested directly.
    pub fn map_channel(&self, value: u8) -> u8 {
        let contrast_factor = 1.0 + self.contrast.clamp(-1.0, 1.0);
        let inv_gamma = 1.0 / self.gamma.max(0.01);
        let mut x = value as f32 / 255.0;
        x = (x - 0.5) * contrast_factor + 0.5;
        x += self.brightness;
        x = x.clamp(0.0, 1.0).powf(inv_gamma);
        (x.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    fn lookup_table(&self) -> [u8; 256] {
        let mut table = [0u8; 256];
        for (value, slot) in table.iter_mut().enumerate() {
            *slot = self.map_channel(value as u8);
        }
        table
    }
}

#[derive(Debug, Clone)]
pub struct DecodeRequest {
    pub request_id: DecodeRequestId,
    pub page_index: usize,
    pub bytes: Vec<u8>,
    pub purpose: DecodePurpose,
    pub target_size: Option<[u32; 2]>,
    pub rotation: Rotation,
    pub adjustments: ImageAdjustments,
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
        decode_bytes(
            &request.bytes,
            request.target_size,
            request.rotation,
            request.adjustments,
        )
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
    rotation: Rotation,
    adjustments: ImageAdjustments,
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

    let image = match rotation {
        Rotation::None => image,
        Rotation::Cw90 => image.rotate90(),
        Rotation::Cw180 => image.rotate180(),
        Rotation::Cw270 => image.rotate270(),
    };

    let mut rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_DECODED_PIXELS {
        return Err(DecodeError::ImageTooLarge { width, height });
    }

    if !adjustments.is_identity() {
        let table = adjustments
            .has_per_channel_adjustment()
            .then(|| adjustments.lookup_table());
        let grayscale_mix = adjustments.grayscale.clamp(0.0, 1.0);
        let value_study = adjustments.value_study;
        let value_bands = adjustments
            .value_bands
            .clamp(VALUE_BANDS_MIN, VALUE_BANDS_MAX);
        for pixel in rgba.pixels_mut() {
            if let Some(table) = &table {
                pixel[0] = table[pixel[0] as usize];
                pixel[1] = table[pixel[1] as usize];
                pixel[2] = table[pixel[2] as usize];
            }
            if value_study {
                let luma = pixel_luma(pixel[0], pixel[1], pixel[2]);
                let levels = value_bands as f32;
                let quantized =
                    ((luma * levels).floor() / (levels - 1.0)).clamp(0.0, 1.0);
                let byte = (quantized * 255.0).round() as u8;
                pixel[0] = byte;
                pixel[1] = byte;
                pixel[2] = byte;
            } else if grayscale_mix > 0.0 {
                let luma = pixel_luma(pixel[0], pixel[1], pixel[2]);
                let gray = (luma * 255.0).round() as u8;
                pixel[0] = mix_channel(pixel[0], gray, grayscale_mix);
                pixel[1] = mix_channel(pixel[1], gray, grayscale_mix);
                pixel[2] = mix_channel(pixel[2], gray, grayscale_mix);
            }
        }
    }

    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        rgba.as_raw(),
    ))
}

fn pixel_luma(r: u8, g: u8, b: u8) -> f32 {
    (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0
}

fn mix_channel(source: u8, target: u8, mix: f32) -> u8 {
    let blended = source as f32 * (1.0 - mix) + target as f32 * mix;
    blended.round().clamp(0.0, 255.0) as u8
}
