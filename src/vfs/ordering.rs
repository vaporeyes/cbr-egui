use std::cmp::Ordering;
use std::path::Path;

const PAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

pub fn is_hidden_metadata_path(path: &str) -> bool {
    path.split('/').any(|segment| {
        segment.is_empty()
            || segment == "__MACOSX"
            || segment.starts_with('.')
            || segment.eq_ignore_ascii_case("thumbs.db")
    })
}

pub fn is_page_image_path(path: &str) -> bool {
    if is_hidden_metadata_path(path) || path.ends_with('/') {
        return false;
    }

    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            PAGE_EXTENSIONS
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
}

pub fn sort_natural(paths: &mut [String]) {
    paths.sort_by(|left, right| natural_cmp(left, right));
}

fn natural_cmp(left: &str, right: &str) -> Ordering {
    match natord::compare(left, right) {
        Ordering::Equal => left.cmp(right),
        ordering => ordering,
    }
}
