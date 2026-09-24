// ABOUTME: Enforces input and raster budgets before archive and document allocation.
// ABOUTME: Shares bounded reads and aspect-preserving render sizes across backends.
use std::io::Read;

use super::ArchiveError;

pub const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_METADATA_BYTES: u64 = 1024 * 1024;
pub const MAX_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_SOURCE_PIXELS: u64 = 100_000_000;
pub const MAX_RASTER_PIXELS: u64 = 8_000_000;
pub const MAX_RASTER_SIDE: u32 = 8192;

pub fn entry_limit(path: &str) -> u64 {
    if path
        .rsplit(['/', '\\'])
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("ComicInfo.xml"))
    {
        MAX_METADATA_BYTES
    } else {
        MAX_ENTRY_BYTES
    }
}

pub fn check_size(size: u64, limit: u64) -> Result<(), ArchiveError> {
    if size > limit {
        Err(ArchiveError::ResourceLimit(format!(
            "{size} exceeds limit {limit}"
        )))
    } else {
        Ok(())
    }
}

pub fn read_bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>, ArchiveError> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    check_size(bytes.len() as u64, limit)?;
    Ok(bytes)
}

pub fn raster_size(
    width: u32,
    height: u32,
    target: Option<[u32; 2]>,
) -> Result<[u32; 2], ArchiveError> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || width > 65_536 || height > 65_536 || pixels > MAX_SOURCE_PIXELS
    {
        return Err(ArchiveError::ResourceLimit(format!(
            "page dimensions {width}x{height}"
        )));
    }
    let [tw, th] = target.unwrap_or([MAX_RASTER_SIDE; 2]);
    let scale = (tw.clamp(1, MAX_RASTER_SIDE) as f64 / width as f64)
        .min(th.clamp(1, MAX_RASTER_SIDE) as f64 / height as f64)
        .min((MAX_RASTER_PIXELS as f64 / pixels as f64).sqrt())
        .min(1.0);
    Ok([
        (width as f64 * scale).floor().max(1.0) as u32,
        (height as f64 * scale).floor().max(1.0) as u32,
    ])
}
