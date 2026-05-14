use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::ImageFormat;
use pdfium_render::prelude::{PdfRenderConfig, Pdfium};

use super::archive::{ArchiveError, ArchiveReader};
use crate::library::models::ArchivePage;

pub struct PdfArchiveReader {
    path: PathBuf,
}

impl PdfArchiveReader {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    fn pdfium() -> Result<Pdfium, ArchiveError> {
        Ok(Pdfium::new(Pdfium::bind_to_system_library().map_err(
            |err| ArchiveError::BackendUnavailable(err.to_string()),
        )?))
    }

    fn page_index(path: &str) -> Result<i32, ArchiveError> {
        let index = path
            .strip_prefix("page_")
            .and_then(|value| value.strip_suffix(".png"))
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))?
            .parse::<i32>()
            .map_err(|_| ArchiveError::NotFound(path.to_owned()))?;
        index
            .checked_sub(1)
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))
    }
}

impl ArchiveReader for PdfArchiveReader {
    fn list_pages(&mut self) -> Result<Vec<ArchivePage>, ArchiveError> {
        let pdfium = Self::pdfium()?;
        let document = pdfium
            .load_pdf_from_file(&self.path, None)
            .map_err(|err| ArchiveError::Read(err.to_string()))?;
        Ok((0..document.pages().len())
            .map(|index| ArchivePage {
                path: format!("page_{}.png", index + 1),
                sort_index: index as usize,
            })
            .collect())
    }

    fn read_page(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError> {
        self.read_entry(path)?
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))
    }

    fn read_entry(&mut self, path: &str) -> Result<Option<Vec<u8>>, ArchiveError> {
        let page_index = Self::page_index(path)?;
        let pdfium = Self::pdfium()?;
        let document = pdfium
            .load_pdf_from_file(&self.path, None)
            .map_err(|err| ArchiveError::Read(err.to_string()))?;
        let page = document
            .pages()
            .get(page_index)
            .map_err(|_| ArchiveError::NotFound(path.to_owned()))?;
        let image = page
            .render_with_config(&PdfRenderConfig::new())
            .map_err(|err| ArchiveError::Read(err.to_string()))?
            .as_image()
            .map_err(|err| ArchiveError::Read(err.to_string()))?;
        let mut cursor = Cursor::new(Vec::new());
        image
            .write_to(&mut cursor, ImageFormat::Png)
            .map_err(|err| ArchiveError::Read(err.to_string()))?;
        Ok(Some(cursor.into_inner()))
    }
}
