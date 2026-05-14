use serde::Deserialize;

use super::errors::MetadataError;
use super::models::ComicMetadata;
use crate::vfs::ArchiveReader;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ComicInfoXml {
    title: Option<String>,
    number: Option<String>,
    writer: Option<String>,
    penciller: Option<String>,
}

pub fn parse_comic_info_xml(bytes: &[u8]) -> Result<ComicMetadata, MetadataError> {
    let xml =
        std::str::from_utf8(bytes).map_err(|err| MetadataError::Malformed(err.to_string()))?;
    let parsed = quick_xml::de::from_str::<ComicInfoXml>(xml)
        .map_err(|err| MetadataError::Malformed(err.to_string()))?;

    Ok(ComicMetadata {
        id: None,
        title: empty_to_none(parsed.title),
        number: empty_to_none(parsed.number),
        writer: empty_to_none(parsed.writer),
        penciller: empty_to_none(parsed.penciller),
    })
}

pub fn read_archive_metadata(
    reader: &mut dyn ArchiveReader,
) -> Result<Option<ComicMetadata>, MetadataError> {
    for path in ["ComicInfo.xml", "comicinfo.xml"] {
        if let Some(bytes) = reader.read_entry(path)? {
            return parse_comic_info_xml(&bytes).map(Some);
        }
    }
    Ok(None)
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}
