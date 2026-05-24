use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use image::imageops::FilterType;
use thiserror::Error;

use crate::vfs::{
    ArchiveError, ArchiveReader, PdfArchiveReader, RarArchiveReader, ZipArchiveReader,
};

use crate::library::models::ArchivePage;

pub const MAX_THUMBNAIL_HEIGHT: u32 = 300;

#[derive(Debug, Error)]
pub enum ThumbnailCacheError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("missing cover page")]
    MissingCover,
    #[error("invalid thumbnail worker pool configuration")]
    InvalidWorkerPool,
    #[error("thumbnail queue is full")]
    QueueFull,
    #[error("thumbnail worker has stopped")]
    WorkerStopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbnailCacheEntry {
    pub source_path: String,
    pub source_fingerprint: String,
    pub cache_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbnailRequest {
    pub source_path: String,
    pub source_fingerprint: String,
    pub cache_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbnailResult {
    pub source_path: String,
    pub cache_path: PathBuf,
    pub outcome: Result<[u32; 2], String>,
}

pub struct ThumbnailWorkerPool {
    sender: Option<Sender<ThumbnailRequest>>,
    receiver: Receiver<ThumbnailResult>,
    handles: Vec<JoinHandle<()>>,
}

impl ThumbnailWorkerPool {
    pub fn start(worker_count: usize, queue_bound: usize) -> Result<Self, ThumbnailCacheError> {
        if worker_count == 0 || queue_bound == 0 {
            return Err(ThumbnailCacheError::InvalidWorkerPool);
        }

        let (sender, request_receiver) = bounded::<ThumbnailRequest>(queue_bound);
        let (result_sender, receiver) = bounded::<ThumbnailResult>(queue_bound);
        let handles = (0..worker_count)
            .map(|_| spawn_thumbnail_worker(request_receiver.clone(), result_sender.clone()))
            .collect();
        drop(result_sender);

        Ok(Self {
            sender: Some(sender),
            receiver,
            handles,
        })
    }

    pub fn submit(&self, request: ThumbnailRequest) -> Result<(), ThumbnailCacheError> {
        let Some(sender) = &self.sender else {
            return Err(ThumbnailCacheError::WorkerStopped);
        };
        sender.try_send(request).map_err(|err| match err {
            TrySendError::Full(_) => ThumbnailCacheError::QueueFull,
            TrySendError::Disconnected(_) => ThumbnailCacheError::WorkerStopped,
        })
    }

    pub fn try_recv(&self) -> Option<ThumbnailResult> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for ThumbnailWorkerPool {
    fn drop(&mut self) {
        self.sender.take();
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

pub fn cache_key_for_source(source_path: &str, source_fingerprint: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source_path.hash(&mut hasher);
    source_fingerprint.hash(&mut hasher);
    format!("{:016x}.png", hasher.finish())
}

pub fn cache_path_for_source(
    cache_root: impl AsRef<Path>,
    source_path: &str,
    source_fingerprint: &str,
) -> PathBuf {
    cache_root
        .as_ref()
        .join(cache_key_for_source(source_path, source_fingerprint))
}

pub fn is_thumbnail_stale(entry: &ThumbnailCacheEntry, current_source_fingerprint: &str) -> bool {
    entry.source_fingerprint != current_source_fingerprint || !entry.cache_path.exists()
}

pub fn cover_request_for_pages(pages: &[ArchivePage]) -> Option<&ArchivePage> {
    pages.iter().min_by_key(|page| page.sort_index)
}

pub fn thumbnail_target_size(width: u32, height: u32) -> [u32; 2] {
    if width == 0 || height == 0 || height <= MAX_THUMBNAIL_HEIGHT {
        return [width, height];
    }

    let scale = MAX_THUMBNAIL_HEIGHT as f32 / height as f32;
    [
        ((width as f32) * scale).round().max(1.0) as u32,
        MAX_THUMBNAIL_HEIGHT,
    ]
}

pub fn write_thumbnail(
    bytes: &[u8],
    cache_path: impl AsRef<Path>,
) -> Result<[u32; 2], ThumbnailCacheError> {
    let image = image::load_from_memory(bytes)?;
    let [target_width, target_height] = thumbnail_target_size(image.width(), image.height());
    let image = if target_height > 0 && target_height < image.height() {
        image.resize(target_width, target_height, FilterType::Lanczos3)
    } else {
        image
    };
    if let Some(parent) = cache_path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    image.save(cache_path)?;
    Ok([image.width(), image.height()])
}

fn spawn_thumbnail_worker(
    request_receiver: Receiver<ThumbnailRequest>,
    result_sender: Sender<ThumbnailResult>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while let Ok(request) = request_receiver.recv() {
            let outcome = read_cover_bytes(Path::new(&request.source_path))
                .map_err(|err| err.to_string())
                .and_then(|bytes| {
                    write_thumbnail(&bytes, &request.cache_path).map_err(|err| err.to_string())
                });
            let result = ThumbnailResult {
                source_path: request.source_path,
                cache_path: request.cache_path,
                outcome,
            };
            if result_sender.send(result).is_err() {
                break;
            }
        }
    })
}

fn read_cover_bytes(archive_path: &Path) -> Result<Vec<u8>, ArchiveError> {
    let mut reader = archive_reader_for_path(archive_path)?;
    let pages = reader.list_pages()?;
    let page = cover_request_for_pages(&pages)
        .ok_or_else(|| ArchiveError::NotFound("cover page".to_owned()))?;
    reader.read_page(&page.path)
}

fn archive_reader_for_path(path: &Path) -> Result<Box<dyn ArchiveReader>, ArchiveError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" | "zip" => Ok(Box::new(ZipArchiveReader::new(path))),
        "cbr" | "rar" => Ok(Box::new(RarArchiveReader::new(path))),
        "pdf" => Ok(Box::new(PdfArchiveReader::new(path))),
        _ => Err(ArchiveError::UnsupportedFormat(path.display().to_string())),
    }
}
