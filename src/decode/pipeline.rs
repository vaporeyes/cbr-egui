// ABOUTME: Decodes raw page image bytes into egui color images for display.
// ABOUTME: Defines cancellation-aware decode request and result payloads.
use std::borrow::Cow;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use eframe::egui;
use image::imageops::FilterType;
use rayon::prelude::*;

use super::error::DecodeError;

const MAX_DECODED_PIXELS: u64 = 100_000_000;
/// Hard bound on either source dimension, enforced by the decoder itself.
const MAX_SOURCE_DIMENSION: u32 = 65_536;
/// Cap on decoder scratch allocation. The `image` default is 512 MiB, which
/// several decode workers can hold simultaneously.
const MAX_DECODE_ALLOC_BYTES: u64 = 256 * 1024 * 1024;
/// Longest edge handed to the GPU. OpenGL implementations commonly cap
/// textures at 16384 px and `load_texture` has no error path, so a page above
/// the driver limit uploads as garbage or fails outright. Pages beyond this
/// are downscaled first.
const MAX_TEXTURE_DIMENSION: u32 = 8_192;

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

/// Where a decode task obtains its compressed image bytes. `ArchivePage` defers
/// the archive read into the worker thread so the GUI thread never blocks on
/// decompression; `Bytes` carries pre-read bytes for the synchronous decode
/// paths that already hold them.
#[derive(Debug, Clone)]
pub enum DecodeSource {
    Bytes(Vec<u8>),
    ArchivePage {
        archive_path: PathBuf,
        page_path: String,
    },
}

#[derive(Debug, Clone)]
pub struct DecodeRequest {
    pub request_id: DecodeRequestId,
    pub page_index: usize,
    pub source: DecodeSource,
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
    let cancel = request.cancellation_token.as_ref();
    let outcome = abort_if_cancelled(cancel).and_then(|()| {
        // Reading the archive happens here, on the worker thread, so page
        // decompression never stalls the GUI frame loop.
        resolve_source_bytes(&request.source).and_then(|bytes| {
            decode_bytes(
                bytes.as_ref(),
                request.target_size,
                request.rotation,
                request.adjustments,
                cancel,
            )
        })
    });
    DecodeResult {
        request_id: request.request_id,
        page_index: request.page_index,
        purpose: request.purpose,
        outcome,
    }
}

fn resolve_source_bytes(source: &DecodeSource) -> Result<Cow<'_, [u8]>, DecodeError> {
    match source {
        DecodeSource::Bytes(bytes) => Ok(Cow::Borrowed(bytes)),
        DecodeSource::ArchivePage {
            archive_path,
            page_path,
        } => crate::vfs::read_page_bytes(archive_path, page_path)
            .map(Cow::Owned)
            .map_err(|err| DecodeError::Image(err.to_string())),
    }
}

fn decode_bytes(
    bytes: &[u8],
    target_size: Option<[u32; 2]>,
    rotation: Rotation,
    adjustments: ImageAdjustments,
    cancel: Option<&CancellationToken>,
) -> Result<egui::ColorImage, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::EmptyBytes);
    }

    // Check the header before decoding. Testing the pixel count after
    // `load_from_memory` would mean the buffer this guard exists to prevent had
    // already been allocated.
    let (source_width, source_height) = limited_reader(bytes)?
        .into_dimensions()
        .map_err(|err| DecodeError::Image(err.to_string()))?;
    if u64::from(source_width) * u64::from(source_height) > MAX_DECODED_PIXELS {
        return Err(DecodeError::ImageTooLarge {
            width: source_width,
            height: source_height,
        });
    }

    // Each stage below is a full-page pass. Re-check between them so a page the
    // reader has already navigated away from stops costing work, instead of
    // running to completion and holding up whatever is on screen now.
    abort_if_cancelled(cancel)?;
    let mut image = limited_reader(bytes)?
        .decode()
        .map_err(|err| DecodeError::Image(err.to_string()))?;

    abort_if_cancelled(cancel)?;
    if let Some([target_width, target_height]) = target_size
        && target_width > 0
        && target_height > 0
        && (image.width() > target_width || image.height() > target_height)
    {
        image = image.resize(target_width, target_height, FilterType::Lanczos3);
    }

    abort_if_cancelled(cancel)?;
    let image = match rotation {
        Rotation::None => image,
        Rotation::Cw90 => image.rotate90(),
        Rotation::Cw180 => image.rotate180(),
        Rotation::Cw270 => image.rotate270(),
    };

    // Downscale past the driver's texture limit before the RGBA buffer is
    // materialised, so nothing unuploadable reaches `load_texture`.
    let image = if image.width() > MAX_TEXTURE_DIMENSION || image.height() > MAX_TEXTURE_DIMENSION {
        image.resize(
            MAX_TEXTURE_DIMENSION,
            MAX_TEXTURE_DIMENSION,
            FilterType::Lanczos3,
        )
    } else {
        image
    };

    abort_if_cancelled(cancel)?;
    let mut rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();

    if !adjustments.is_identity() {
        let table = adjustments
            .has_per_channel_adjustment()
            .then(|| adjustments.lookup_table());
        let grayscale_mix = adjustments.grayscale.clamp(0.0, 1.0);
        let value_study = adjustments.value_study;
        let value_bands = adjustments
            .value_bands
            .clamp(VALUE_BANDS_MIN, VALUE_BANDS_MAX);
        // Multi-megapixel pages make this per-pixel pass the dominant decode cost,
        // so spread the independent pixel math across the rayon thread pool.
        let mut slice = rgba.as_flat_samples_mut();
        slice
            .as_mut_slice()
            .par_chunks_exact_mut(4)
            .for_each(|pixel| {
                if let Some(table) = &table {
                    pixel[0] = table[pixel[0] as usize];
                    pixel[1] = table[pixel[1] as usize];
                    pixel[2] = table[pixel[2] as usize];
                }
                if value_study {
                    let luma = pixel_luma(pixel[0], pixel[1], pixel[2]);
                    let levels = value_bands as f32;
                    let quantized = ((luma * levels).floor() / (levels - 1.0)).clamp(0.0, 1.0);
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
            });
    }

    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        rgba.as_raw(),
    ))
}

fn abort_if_cancelled(cancel: Option<&CancellationToken>) -> Result<(), DecodeError> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return Err(DecodeError::Cancelled);
    }
    Ok(())
}

/// Builds an image reader with allocation and dimension limits applied. The
/// `image` defaults leave dimensions unbounded and allow 512 MiB of decoder
/// scratch per call, which several decode workers can hold at once.
fn limited_reader(bytes: &[u8]) -> Result<image::ImageReader<Cursor<&[u8]>>, DecodeError> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|err| DecodeError::Image(err.to_string()))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_DIMENSION);
    limits.max_image_height = Some(MAX_SOURCE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    reader.limits(limits);
    Ok(reader)
}

fn pixel_luma(r: u8, g: u8, b: u8) -> f32 {
    (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0
}

fn mix_channel(source: u8, target: u8, mix: f32) -> u8 {
    let blended = source as f32 * (1.0 - mix) + target as f32 * mix;
    blended.round().clamp(0.0, 255.0) as u8
}
