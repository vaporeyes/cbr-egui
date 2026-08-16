use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use super::errors::LibraryError;
use super::models::{
    Bookmark, Comic, ComicAvailability, ComicInput, ComicMetadata, ComicMetadataDisplay, Folder,
    LibraryComicRow, Progress,
};

pub struct LibraryStorage {
    connection: Connection,
}

impl LibraryStorage {
    pub fn open(path: &std::path::Path) -> Result<Self, LibraryError> {
        let connection = Connection::open(path)?;
        // Reading position is checkpointed while the reader is in use, so
        // writes want to be cheap and must not fail outright if another handle
        // holds the database for a moment.
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        let storage = Self { connection };
        storage.initialize_schema()?;
        Ok(storage)
    }

    pub fn initialize_schema(&self) -> Result<(), LibraryError> {
        self.connection.execute_batch(
            "
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS folders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                parent_id INTEGER NULL,
                FOREIGN KEY(parent_id) REFERENCES folders(id)
            );

            CREATE TABLE IF NOT EXISTS metadata (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                series TEXT NULL,
                title TEXT NULL,
                number TEXT NULL,
                writer TEXT NULL,
                penciller TEXT NULL
            );

            CREATE TABLE IF NOT EXISTS comics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                hash TEXT NOT NULL,
                page_count INTEGER NOT NULL,
                metadata_id INTEGER NULL,
                availability INTEGER NOT NULL DEFAULT 1,
                thumbnail_key TEXT NULL
            );

            CREATE TABLE IF NOT EXISTS progress (
                comic_id INTEGER PRIMARY KEY,
                current_page INTEGER NOT NULL,
                is_read INTEGER NOT NULL,
                updated_at INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(comic_id) REFERENCES comics(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS bookmarks (
                comic_id INTEGER NOT NULL,
                page_index INTEGER NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (comic_id, page_index),
                FOREIGN KEY(comic_id) REFERENCES comics(id) ON DELETE CASCADE
            );

            -- delete_metadata_if_unreferenced counts comics per metadata row,
            -- once per comic during a reconcile. Without this the count is a
            -- full table scan and the reconcile becomes quadratic.
            CREATE INDEX IF NOT EXISTS idx_comics_metadata_id ON comics(metadata_id);
            ",
        )?;
        self.add_column_if_missing("metadata", "series", "TEXT NULL")?;
        self.add_column_if_missing("comics", "availability", "INTEGER NOT NULL DEFAULT 1")?;
        self.add_column_if_missing("comics", "thumbnail_key", "TEXT NULL")?;
        self.add_column_if_missing("progress", "updated_at", "INTEGER NOT NULL DEFAULT 0")?;
        Ok(())
    }

    /// `table`, `column`, and `definition` are compile-time literals from
    /// `initialize_schema`; none of them are interpolated from runtime input.
    fn add_column_if_missing(
        &self,
        table: &str,
        column: &str,
        definition: &str,
    ) -> Result<(), LibraryError> {
        // Ask the schema rather than matching on the text of the error. The
        // "duplicate column name" wording is not a stable interface, and
        // mistaking it for a real failure turns a no-op migration into a
        // startup failure that leaves the app without a database.
        let already_present = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
            params![table, column],
            |row| row.get::<_, bool>(0),
        )?;
        if already_present {
            return Ok(());
        }

        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
        self.connection.execute_batch(&sql)?;
        Ok(())
    }

    /// Begins a transaction on the shared connection. All other storage methods
    /// run their statements on the same `&self` connection, so they participate
    /// in this transaction until it is committed (or rolled back on drop). This
    /// collapses a multi-comic reconciliation into a single fsync instead of one
    /// per INSERT/UPDATE.
    pub fn transaction(&self) -> Result<rusqlite::Transaction<'_>, LibraryError> {
        Ok(self.connection.unchecked_transaction()?)
    }

    pub fn table_exists(&self, table: &str) -> Result<bool, LibraryError> {
        let exists = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get::<_, bool>(0),
        )?;
        Ok(exists)
    }

    pub fn create_folder(
        &self,
        path: &str,
        parent_id: Option<i64>,
    ) -> Result<Folder, LibraryError> {
        if let Some(id) = parent_id {
            let exists = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM folders WHERE id = ?1)",
                [id],
                |row| row.get::<_, bool>(0),
            )?;
            if !exists {
                return Err(LibraryError::InvalidParent(id));
            }
        }

        self.connection.execute(
            "INSERT INTO folders (path, parent_id) VALUES (?1, ?2)
             ON CONFLICT(path) DO UPDATE SET parent_id = excluded.parent_id",
            params![path, parent_id],
        )?;

        self.get_folder_by_path(path)?
            .ok_or(LibraryError::Database(rusqlite::Error::QueryReturnedNoRows))
    }

    pub fn get_folder(&self, id: i64) -> Result<Option<Folder>, LibraryError> {
        self.connection
            .query_row(
                "SELECT id, path, parent_id FROM folders WHERE id = ?1",
                [id],
                folder_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_folder_by_path(&self, path: &str) -> Result<Option<Folder>, LibraryError> {
        self.connection
            .query_row(
                "SELECT id, path, parent_id FROM folders WHERE path = ?1",
                [path],
                folder_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert_comic(&self, input: &ComicInput) -> Result<Comic, LibraryError> {
        self.connection.execute(
            "INSERT INTO comics (path, hash, page_count, metadata_id, availability) VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT(path) DO UPDATE SET
                hash = excluded.hash,
                page_count = excluded.page_count,
                metadata_id = excluded.metadata_id,
                availability = 1",
            params![input.path, input.hash, input.page_count, input.metadata_id],
        )?;

        self.get_comic_by_path(&input.path)?
            .ok_or(LibraryError::Database(rusqlite::Error::QueryReturnedNoRows))
    }

    pub fn upsert_metadata(
        &self,
        existing_id: Option<i64>,
        input: &ComicMetadata,
    ) -> Result<ComicMetadata, LibraryError> {
        if let Some(id) = existing_id {
            let updated = self.connection.execute(
                "UPDATE metadata
                 SET series = ?2, title = ?3, number = ?4, writer = ?5, penciller = ?6
                 WHERE id = ?1",
                params![
                    id,
                    input.series,
                    input.title,
                    input.number,
                    input.writer,
                    input.penciller
                ],
            )?;
            if updated > 0 {
                return Ok(ComicMetadata {
                    id: Some(id),
                    series: input.series.clone(),
                    title: input.title.clone(),
                    number: input.number.clone(),
                    writer: input.writer.clone(),
                    penciller: input.penciller.clone(),
                });
            }
        }

        self.connection.execute(
            "INSERT INTO metadata (series, title, number, writer, penciller) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                input.series,
                input.title,
                input.number,
                input.writer,
                input.penciller
            ],
        )?;
        let id = self.connection.last_insert_rowid();
        Ok(ComicMetadata {
            id: Some(id),
            series: input.series.clone(),
            title: input.title.clone(),
            number: input.number.clone(),
            writer: input.writer.clone(),
            penciller: input.penciller.clone(),
        })
    }

    pub fn delete_metadata_if_unreferenced(&self, metadata_id: i64) -> Result<(), LibraryError> {
        let refs = self.connection.query_row(
            "SELECT COUNT(*) FROM comics WHERE metadata_id = ?1",
            [metadata_id],
            |row| row.get::<_, i64>(0),
        )?;
        if refs == 0 {
            self.connection
                .execute("DELETE FROM metadata WHERE id = ?1", [metadata_id])?;
        }
        Ok(())
    }

    pub fn get_comic(&self, id: i64) -> Result<Option<Comic>, LibraryError> {
        self.connection
            .query_row(
                comic_select_sql("WHERE id = ?1").as_str(),
                [id],
                comic_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_comic_row(&self, id: i64) -> Result<Option<LibraryComicRow>, LibraryError> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                c.id,
                c.path,
                c.hash,
                c.page_count,
                c.metadata_id,
                c.availability,
                c.thumbnail_key,
                m.series,
                m.title,
                m.number,
                m.writer,
                m.penciller
            FROM comics c
            LEFT JOIN metadata m ON m.id = c.metadata_id
            WHERE c.id = ?1
            ",
        )?;
        statement
            .query_row([id], library_comic_row_from_row)
            .optional()
            .map_err(Into::into)
    }

    pub fn get_comic_by_path(&self, path: &str) -> Result<Option<Comic>, LibraryError> {
        self.connection
            .query_row(
                comic_select_sql("WHERE path = ?1").as_str(),
                [path],
                comic_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_comics(&self) -> Result<Vec<Comic>, LibraryError> {
        let sql = comic_select_sql("ORDER BY path");
        let mut statement = self.connection.prepare(&sql)?;
        let comics = statement
            .query_map([], comic_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(comics)
    }

    pub fn list_library_comic_rows(&self) -> Result<Vec<LibraryComicRow>, LibraryError> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                c.id,
                c.path,
                c.hash,
                c.page_count,
                c.metadata_id,
                c.availability,
                c.thumbnail_key,
                m.series,
                m.title,
                m.number,
                m.writer,
                m.penciller
            FROM comics c
            LEFT JOIN metadata m ON m.id = c.metadata_id
            ORDER BY c.path
            ",
        )?;
        let rows = statement
            .query_map([], library_comic_row_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_comic_availability(
        &self,
        path: &str,
        availability: ComicAvailability,
    ) -> Result<(), LibraryError> {
        self.connection.execute(
            "UPDATE comics SET availability = ?2 WHERE path = ?1",
            params![path, availability.as_i64()],
        )?;
        Ok(())
    }

    pub fn set_thumbnail_key(
        &self,
        path: &str,
        thumbnail_key: Option<&str>,
    ) -> Result<(), LibraryError> {
        self.connection.execute(
            "UPDATE comics SET thumbnail_key = ?2 WHERE path = ?1",
            params![path, thumbnail_key],
        )?;
        Ok(())
    }

    pub fn save_progress(
        &self,
        comic_id: i64,
        current_page: u32,
        is_read: bool,
    ) -> Result<Progress, LibraryError> {
        let updated_at = current_unix_timestamp();
        self.connection.execute(
            "INSERT INTO progress (comic_id, current_page, is_read, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(comic_id) DO UPDATE SET
                current_page = excluded.current_page,
                is_read = excluded.is_read,
                updated_at = excluded.updated_at",
            params![comic_id, current_page, is_read, updated_at],
        )?;

        self.get_progress(comic_id)?
            .ok_or(LibraryError::Database(rusqlite::Error::QueryReturnedNoRows))
    }

    pub fn get_progress(&self, comic_id: i64) -> Result<Option<Progress>, LibraryError> {
        self.connection
            .query_row(
                "SELECT comic_id, current_page, is_read, updated_at FROM progress WHERE comic_id = ?1",
                [comic_id],
                progress_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    /// Most recently updated progress row for an available, readable comic.
    /// Done as one join rather than listing every comic and querying progress
    /// per row, which cost one statement per comic in the library at startup.
    pub fn last_read_comic(&self) -> Result<Option<(Comic, Progress)>, LibraryError> {
        self.connection
            .query_row(
                "SELECT c.id, c.path, c.hash, c.page_count, c.metadata_id, c.availability,
                        c.thumbnail_key, p.comic_id, p.current_page, p.is_read, p.updated_at
                 FROM comics c
                 JOIN progress p ON p.comic_id = c.id
                 WHERE c.availability = 1 AND c.page_count > 0
                 ORDER BY p.updated_at DESC, c.id DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok((
                        comic_from_row(row)?,
                        Progress {
                            comic_id: row.get(7)?,
                            current_page: row.get(8)?,
                            is_read: row.get(9)?,
                            updated_at: row.get(10)?,
                        },
                    ))
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_read_comic_ids(&self) -> Result<std::collections::HashSet<i64>, LibraryError> {
        let mut statement = self
            .connection
            .prepare("SELECT comic_id FROM progress WHERE is_read = 1")?;
        let ids = statement
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<std::collections::HashSet<_>, _>>()?;
        Ok(ids)
    }

    pub fn list_progress(&self) -> Result<std::collections::HashMap<i64, Progress>, LibraryError> {
        let mut statement = self.connection.prepare(
            "SELECT comic_id, current_page, is_read, updated_at FROM progress",
        )?;
        let progress_iter = statement.query_map([], progress_from_row)?;
        let mut map = std::collections::HashMap::new();
        for progress in progress_iter {
            let progress = progress?;
            map.insert(progress.comic_id, progress);
        }
        Ok(map)
    }

    pub fn progress_count_for_comic(&self, comic_id: i64) -> Result<u32, LibraryError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM progress WHERE comic_id = ?1",
            [comic_id],
            |row| row.get::<_, i64>(0),
        )?;
        i64_to_u32("progress_count", count)
    }

    pub fn add_bookmark(&self, comic_id: i64, page_index: u32) -> Result<(), LibraryError> {
        self.connection.execute(
            "INSERT INTO bookmarks (comic_id, page_index, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(comic_id, page_index) DO NOTHING",
            params![comic_id, page_index, current_unix_timestamp()],
        )?;
        Ok(())
    }

    pub fn remove_bookmark(&self, comic_id: i64, page_index: u32) -> Result<(), LibraryError> {
        self.connection.execute(
            "DELETE FROM bookmarks WHERE comic_id = ?1 AND page_index = ?2",
            params![comic_id, page_index],
        )?;
        Ok(())
    }

    pub fn is_bookmarked(&self, comic_id: i64, page_index: u32) -> Result<bool, LibraryError> {
        let exists = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM bookmarks WHERE comic_id = ?1 AND page_index = ?2)",
            params![comic_id, page_index],
            |row| row.get::<_, bool>(0),
        )?;
        Ok(exists)
    }

    pub fn list_bookmarks(&self, comic_id: i64) -> Result<Vec<Bookmark>, LibraryError> {
        let mut statement = self.connection.prepare(
            "SELECT comic_id, page_index, created_at FROM bookmarks
             WHERE comic_id = ?1 ORDER BY page_index",
        )?;
        let bookmarks = statement
            .query_map([comic_id], bookmark_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(bookmarks)
    }

    pub fn delete_comic(&self, comic_id: i64) -> Result<Option<Comic>, LibraryError> {
        let Some(comic) = self.get_comic(comic_id)? else {
            return Ok(None);
        };
        self.connection
            .execute("DELETE FROM comics WHERE id = ?1", [comic_id])?;
        Ok(Some(comic))
    }

    pub fn purge_unavailable_comics(&self) -> Result<usize, LibraryError> {
        let deleted = self
            .connection
            .execute("DELETE FROM comics WHERE availability = 0", [])?;
        Ok(deleted)
    }
}

fn folder_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: row.get(0)?,
        path: row.get(1)?,
        parent_id: row.get(2)?,
    })
}

fn comic_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Comic> {
    let page_count = row.get::<_, u32>(3)?;
    Ok(Comic {
        id: row.get(0)?,
        path: row.get(1)?,
        hash: row.get(2)?,
        page_count,
        metadata_id: row.get(4)?,
        availability: ComicAvailability::from_i64(row.get::<_, i64>(5)?),
        thumbnail_key: row.get(6)?,
    })
}

fn library_comic_row_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryComicRow> {
    let page_count = row.get::<_, u32>(3)?;
    Ok(LibraryComicRow {
        comic: Comic {
            id: row.get(0)?,
            path: row.get(1)?,
            hash: row.get(2)?,
            page_count,
            metadata_id: row.get(4)?,
            availability: ComicAvailability::from_i64(row.get::<_, i64>(5)?),
            thumbnail_key: row.get(6)?,
        },
        metadata: ComicMetadataDisplay {
            series: row.get(7)?,
            title: row.get(8)?,
            number: row.get(9)?,
            writer: row.get(10)?,
            penciller: row.get(11)?,
        },
    })
}

fn comic_select_sql(suffix: &str) -> String {
    format!(
        "SELECT id, path, hash, page_count, metadata_id, availability, thumbnail_key FROM comics {suffix}"
    )
}

fn bookmark_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Bookmark> {
    Ok(Bookmark {
        comic_id: row.get(0)?,
        page_index: row.get(1)?,
        created_at: row.get(2)?,
    })
}

fn progress_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Progress> {
    Ok(Progress {
        comic_id: row.get(0)?,
        current_page: row.get(1)?,
        is_read: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn i64_to_u32(field: &'static str, value: i64) -> Result<u32, LibraryError> {
    u32::try_from(value).map_err(|_| LibraryError::IntegerConversion { field, value })
}

fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}
