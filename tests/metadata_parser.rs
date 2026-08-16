use cbr_egui::library::ArchivePage;
use cbr_egui::library::{
    ComicMetadata, MetadataError, document_metadata, parse_comic_info_xml, read_archive_metadata,
};
use cbr_egui::vfs::{ArchiveError, ArchiveReader, build_pages};

#[test]
fn parses_valid_comic_info_fields() {
    let metadata = parse_comic_info_xml(
        br#"<ComicInfo><Series>Saga</Series><Title>Chapter One</Title><Number>1</Number><Writer>Brian K. Vaughan</Writer><Penciller>Fiona Staples</Penciller></ComicInfo>"#,
    )
    .expect("valid metadata");

    assert_eq!(metadata.series.as_deref(), Some("Saga"));
    assert_eq!(metadata.title.as_deref(), Some("Chapter One"));
    assert_eq!(metadata.number.as_deref(), Some("1"));
    assert_eq!(metadata.writer.as_deref(), Some("Brian K. Vaughan"));
    assert_eq!(metadata.penciller.as_deref(), Some("Fiona Staples"));
}

#[test]
fn parses_partial_metadata_as_empty_optional_fields() {
    let metadata = parse_comic_info_xml(br#"<ComicInfo><Title>Only Title</Title></ComicInfo>"#)
        .expect("partial metadata");

    assert_eq!(metadata.title.as_deref(), Some("Only Title"));
    assert_eq!(metadata.series, None);
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
        ..FixtureReader::default()
    };

    let error = read_archive_metadata(&mut reader).expect_err("malformed xml");
    let pages = reader.list_pages().expect("page listing");

    assert!(matches!(error, MetadataError::Malformed(_)));
    assert_eq!(
        pages.into_iter().map(|page| page.path).collect::<Vec<_>>(),
        ["page_1.jpg", "page_2.jpg"]
    );
}

#[test]
fn document_title_and_author_become_comic_metadata() {
    let metadata = document_metadata(
        Some("The Hunting of the Snark".to_owned()),
        Some("Lewis Carroll".to_owned()),
    )
    .expect("metadata");

    assert_eq!(metadata.title.as_deref(), Some("The Hunting of the Snark"));
    assert_eq!(metadata.writer.as_deref(), Some("Lewis Carroll"));
    // PDF and DjVu describe a document, not an issue of a series, so these
    // stay empty rather than being guessed at from the title.
    assert_eq!(metadata.series, None);
    assert_eq!(metadata.number, None);
    assert_eq!(metadata.penciller, None);
}

#[test]
fn blank_document_metadata_counts_as_absent() {
    assert_eq!(document_metadata(None, None), None);
    // Writers commonly leave empty strings in the metadata block; those should
    // not become an entry that renders as a blank row in the info panel.
    assert_eq!(
        document_metadata(Some(String::new()), Some("  ".to_owned())),
        None
    );
    assert!(document_metadata(Some("  Titled  ".to_owned()), None).is_some());
    // Values are trimmed on the way in.
    assert_eq!(
        document_metadata(Some("  Titled  ".to_owned()), None).and_then(|metadata| metadata.title),
        Some("Titled".to_owned())
    );
}

#[test]
fn comic_info_wins_over_document_metadata() {
    let mut reader = FixtureReader {
        metadata: Some(
            br#"<ComicInfo><Series>Saga</Series><Writer>Brian K. Vaughan</Writer></ComicInfo>"#
                .to_vec(),
        ),
        document: document_metadata(Some("Scan 042".to_owned()), Some("Unknown".to_owned())),
        ..FixtureReader::default()
    };

    let metadata = read_archive_metadata(&mut reader)
        .expect("metadata lookup")
        .expect("metadata present");

    // A ComicInfo.xml was authored for this comic specifically, so it describes
    // it better than whatever the containing document happens to carry.
    assert_eq!(metadata.series.as_deref(), Some("Saga"));
    assert_eq!(metadata.writer.as_deref(), Some("Brian K. Vaughan"));
}

#[test]
fn document_metadata_is_used_when_there_is_no_comic_info() {
    let mut reader = FixtureReader {
        document: document_metadata(Some("Scan 042".to_owned()), Some("A. Scanner".to_owned())),
        ..FixtureReader::default()
    };

    let metadata = read_archive_metadata(&mut reader)
        .expect("metadata lookup")
        .expect("metadata present");

    assert_eq!(metadata.title.as_deref(), Some("Scan 042"));
    assert_eq!(metadata.writer.as_deref(), Some("A. Scanner"));
}

#[derive(Default)]
struct FixtureReader {
    metadata: Option<Vec<u8>>,
    document: Option<ComicMetadata>,
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

    fn document_metadata(&mut self) -> Result<Option<ComicMetadata>, ArchiveError> {
        Ok(self.document.clone())
    }
}

#[test]
fn archive_formats_report_no_document_metadata_by_default() {
    // A zip or rar has no document-level metadata of its own, so the trait
    // default must leave the ComicInfo.xml path as the only source.
    struct BareReader;
    impl ArchiveReader for BareReader {
        fn list_pages(&mut self) -> Result<Vec<ArchivePage>, ArchiveError> {
            Ok(Vec::new())
        }
        fn read_page(&mut self, path: &str) -> Result<Vec<u8>, ArchiveError> {
            Err(ArchiveError::NotFound(path.to_owned()))
        }
        fn read_entry(&mut self, _path: &str) -> Result<Option<Vec<u8>>, ArchiveError> {
            Ok(None)
        }
    }

    assert_eq!(
        BareReader.document_metadata().expect("default metadata"),
        None
    );
    assert_eq!(
        read_archive_metadata(&mut BareReader).expect("metadata lookup"),
        None
    );
}
