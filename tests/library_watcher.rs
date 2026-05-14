use std::path::PathBuf;
use std::time::Duration;

use cbr_egui::library::coalesce_watch_events;

#[test]
fn watcher_coalesces_duplicate_paths() {
    let batch = coalesce_watch_events(
        "/library",
        [
            PathBuf::from("/library/book.cbz"),
            PathBuf::from("/library/book.cbz"),
            PathBuf::from("/library/other.cbz"),
        ],
        Duration::from_millis(250),
    );

    assert_eq!(batch.changed_paths.len(), 2);
    assert!(!batch.full_reconciliation);
}

#[test]
fn watcher_burst_schedules_full_reconciliation() {
    let paths = (0..40)
        .map(|index| PathBuf::from(format!("/library/book_{index}.cbz")))
        .collect::<Vec<_>>();

    let batch = coalesce_watch_events("/library", paths, Duration::from_millis(250));

    assert!(batch.full_reconciliation);
}
