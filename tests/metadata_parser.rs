use cbr_egui::library::ArchivePage;
use cbr_egui::library::{MetadataError, parse_comic_info_xml, read_archive_metadata};
use cbr_egui::vfs::{ArchiveError, ArchiveReader, build_pages};

#[test]
fn parses_valid_comic_info_fields() {
    let metadata = parse_comic_info_xml(
        br#"<ComicInfo><Title>Saga</Title><Number>1</Number><Writer>Brian K. Vaughan</Writer><Penciller>Fiona Staples</Penciller></ComicInfo>"#,
    )
    .expect("valid metadata");

    assert_eq!(metadata.title.as_deref(), Some("Saga"));
    assert_eq!(metadata.number.as_deref(), Some("1"));
    assert_eq!(metadata.writer.as_deref(), Some("Brian K. Vaughan"));
    assert_eq!(metadata.penciller.as_deref(), Some("Fiona Staples"));
}

#[test]
fn parses_partial_metadata_as_empty_optional_fields() {
    let metadata = parse_comic_info_xml(br#"<ComicInfo><Title>Only Title</Title></ComicInfo>"#)
        .expect("partial metadata");

    assert_eq!(metadata.title.as_deref(), Some("Only Title"));
    assert_eq!(metadata.number, None);
    assert_eq!(metadata.writer, None);
    assert_eq!(metadata.penciller, None);
}

#[test]
fn malformed_metadata_is_recoverable() {
    let error = parse_comic_info_xml(br#"<ComicInfo><Title>Broken</ComicInfo>"#)
        .expect_err("malformed xml");

    assert!(matches!(error, MetadataError::Malformed(_)));
}

#[test]
fn missing_archive_metadata_returns_none() {
    let mut reader = FixtureReader::default();

    assert_eq!(
        read_archive_metadata(&mut reader).expect("metadata lookup"),
        None
    );
}

#[test]
fn malformed_metadata_does_not_block_page_listing() {
    let mut reader = FixtureReader {
        metadata: Some(br#"<ComicInfo><Title>Broken</ComicInfo>"#.to_vec()),
        pages: vec!["page_2.jpg".to_owned(), "page_1.jpg".to_owned()],
    };

    let error = read_archive_metadata(&mut reader).expect_err("malformed xml");
    let pages = reader.list_pages().expect("page listing");

    assert!(matches!(error, MetadataError::Malformed(_)));
    assert_eq!(
        pages.into_iter().map(|page| page.path).collect::<Vec<_>>(),
        ["page_1.jpg", "page_2.jpg"]
    );
}

#[derive(Default)]
struct FixtureReader {
    metadata: Option<Vec<u8>>,
    pages: Vec<String>,
}

impl ArchiveReader for FixtureReader {
    fn list_pages(&mut self) -> Result<Vec<ArchivePage>, ArchiveError> {
        Ok(build_pages(self.pages.clone()))
    }

    fn read_page(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError> {
        self.read_entry(path)?
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))
    }

    fn read_entry(&mut self, path: &str) -> Result<Option<Vec<u8>>, ArchiveError> {
        if path.eq_ignore_ascii_case("ComicInfo.xml") {
            return Ok(self.metadata.clone());
        }
        Ok(None)
    }
}
