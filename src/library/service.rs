use std::path::Path;

use super::errors::LibraryError;
use super::models::{
    Comic, ComicAvailability, ComicInput, ComicMetadata, ComicMetadataDisplay, Folder,
    LibraryGridItem, Progress, ThumbnailStatus,
};
use super::scanner::ScannedComic;
use super::storage::LibraryStorage;

pub struct LibraryService {
    storage: LibraryStorage,
}

impl LibraryService {
    pub fn initialize(db_path: &Path) -> Result<Self, LibraryError> {
        Ok(Self {
            storage: LibraryStorage::open(db_path)?,
        })
    }

    pub fn create_folder(
        &self,
        path: &str,
        parent_id: Option<i64>,
    ) -> Result<Folder, LibraryError> {
        self.storage.create_folder(path, parent_id)
    }

    pub fn get_folder(&self, id: i64) -> Result<Option<Folder>, LibraryError> {
        self.storage.get_folder(id)
    }

    pub fn upsert_comic(&self, input: ComicInput) -> Result<Comic, LibraryError> {
        self.storage.upsert_comic(&input)
    }

    pub fn insert_metadata(&self, input: &ComicMetadata) -> Result<ComicMetadata, LibraryError> {
        self.storage.upsert_metadata(None, input)
    }

    pub fn get_comic(&self, id: i64) -> Result<Option<Comic>, LibraryError> {
        self.storage.get_comic(id)
    }

    pub fn list_comics(&self) -> Result<Vec<Comic>, LibraryError> {
        self.storage.list_comics()
    }

    pub fn set_comic_availability(
        &self,
        path: &str,
        availability: ComicAvailability,
    ) -> Result<(), LibraryError> {
        self.storage.set_comic_availability(path, availability)
    }

    pub fn set_thumbnail_key(
        &self,
        path: &str,
        thumbnail_key: Option<&str>,
    ) -> Result<(), LibraryError> {
        self.storage.set_thumbnail_key(path, thumbnail_key)
    }

    pub fn reconcile_scanned_comics(
        &self,
        scanned: &[ScannedComic],
    ) -> Result<Vec<Comic>, LibraryError> {
        for comic in scanned {
            let existing = self.storage.get_comic_by_path(&comic.path)?;
            let previous_metadata_id = existing.as_ref().and_then(|comic| comic.metadata_id);
            let metadata_id = comic
                .metadata
                .as_ref()
                .map(|metadata| self.storage.upsert_metadata(previous_metadata_id, metadata))
                .transpose()?
                .and_then(|metadata| metadata.id);
            self.upsert_comic(ComicInput {
                path: comic.path.clone(),
                hash: comic.fingerprint.clone(),
                page_count: comic.page_count,
                metadata_id,
            })?;
            if let Some(previous_metadata_id) = previous_metadata_id
                && Some(previous_metadata_id) != metadata_id
            {
                self.storage
                    .delete_metadata_if_unreferenced(previous_metadata_id)?;
            }
        }

        let scanned_paths = scanned
            .iter()
            .map(|comic| comic.path.as_str())
            .collect::<std::collections::HashSet<_>>();
        for comic in self.list_comics()? {
            if !scanned_paths.contains(comic.path.as_str()) {
                self.set_comic_availability(&comic.path, ComicAvailability::Unavailable)?;
            }
        }

        self.list_comics()
    }

    pub fn library_grid_items(&self) -> Result<Vec<LibraryGridItem>, LibraryError> {
        Ok(self
            .storage
            .list_library_comic_rows()?
            .into_iter()
            .map(|row| {
                let comic = row.comic;
                let folder = folder_parts(&comic.path);
                let series = clean_string(row.metadata.series.clone());
                LibraryGridItem {
                    comic_id: comic.id,
                    title: std::path::Path::new(&comic.path)
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or(&comic.path)
                        .to_owned(),
                    subtitle: metadata_subtitle(&row.metadata),
                    path: comic.path,
                    source_fingerprint: comic.hash,
                    page_count: comic.page_count,
                    thumbnail_status: comic
                        .thumbnail_key
                        .map(|cache_path| ThumbnailStatus::Ready { cache_path })
                        .unwrap_or(ThumbnailStatus::Missing),
                    availability: comic.availability,
                    series_key: series.as_deref().map(normalize_group_key),
                    series,
                    folder_label: folder.clone().map(|(label, _)| label),
                    folder_key: folder.map(|(_, key)| key),
                }
            })
            .collect())
    }

    pub fn save_progress(
        &self,
        comic_id: i64,
        current_page: u32,
        is_read: bool,
    ) -> Result<Progress, LibraryError> {
        self.storage.save_progress(comic_id, current_page, is_read)
    }

    pub fn get_progress(&self, comic_id: i64) -> Result<Option<Progress>, LibraryError> {
        self.storage.get_progress(comic_id)
    }

    pub fn last_read_comic(&self) -> Result<Option<(Comic, Progress)>, LibraryError> {
        let mut candidates = self
            .list_comics()?
            .into_iter()
            .filter(|comic| comic.availability == ComicAvailability::Available)
            .filter_map(|comic| {
                self.get_progress(comic.id)
                    .ok()
                    .flatten()
                    .map(|progress| (comic, progress))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, progress)| progress.updated_at);
        Ok(candidates.pop())
    }

    pub fn purge_unavailable_comics(&self) -> Result<usize, LibraryError> {
        self.storage.purge_unavailable_comics()
    }

    pub fn table_exists(&self, table: &str) -> Result<bool, LibraryError> {
        self.storage.table_exists(table)
    }

    pub fn progress_count_for_comic(&self, comic_id: i64) -> Result<u32, LibraryError> {
        self.storage.progress_count_for_comic(comic_id)
    }
}

pub fn metadata_subtitle(metadata: &ComicMetadataDisplay) -> Option<String> {
    let number = clean_string(metadata.number.clone());
    let creator =
        clean_string(metadata.writer.clone()).or_else(|| clean_string(metadata.penciller.clone()));

    match (number, creator) {
        (Some(number), Some(creator)) => Some(format!("Issue #{number} - {creator}")),
        (Some(number), None) => Some(format!("Issue #{number}")),
        (None, Some(creator)) => Some(creator),
        (None, None) => None,
    }
}

pub fn normalize_group_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn clean_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn folder_parts(path: &str) -> Option<(String, String)> {
    let parent = std::path::Path::new(path).parent()?;
    let key = parent.to_string_lossy().trim().to_owned();
    if key.is_empty() {
        return None;
    }
    let label = parent
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|| key.clone());
    Some((label, key))
}
