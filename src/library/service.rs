use std::path::Path;

use super::errors::LibraryError;
use super::models::{
    Comic, ComicAvailability, ComicInput, Folder, LibraryGridItem, Progress, ThumbnailStatus,
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
            self.upsert_comic(ComicInput {
                path: comic.path.clone(),
                hash: comic.fingerprint.clone(),
                page_count: comic.page_count,
                metadata_id: None,
            })?;
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
            .list_comics()?
            .into_iter()
            .map(|comic| LibraryGridItem {
                comic_id: comic.id,
                title: std::path::Path::new(&comic.path)
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&comic.path)
                    .to_owned(),
                path: comic.path,
                page_count: comic.page_count,
                thumbnail_status: comic
                    .thumbnail_key
                    .map(|cache_path| ThumbnailStatus::Ready { cache_path })
                    .unwrap_or(ThumbnailStatus::Missing),
                availability: comic.availability,
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
        candidates.sort_by_key(|(_, progress)| progress.current_page);
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
