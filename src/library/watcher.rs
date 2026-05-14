use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, unbounded};
use notify::{RecursiveMode, Watcher};

use crate::library::errors::LibraryError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchStatus {
    Idle,
    Watching,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct WatchEventBatch {
    pub root_path: PathBuf,
    pub changed_paths: BTreeSet<PathBuf>,
    pub received_at: Instant,
    pub settle_deadline: Instant,
    pub full_reconciliation: bool,
}

impl WatchEventBatch {
    pub fn new(root_path: impl Into<PathBuf>, settle_for: Duration) -> Self {
        let received_at = Instant::now();
        Self {
            root_path: root_path.into(),
            changed_paths: BTreeSet::new(),
            received_at,
            settle_deadline: received_at + settle_for,
            full_reconciliation: false,
        }
    }

    pub fn push_path(&mut self, path: impl Into<PathBuf>, settle_for: Duration) {
        self.changed_paths.insert(path.into());
        self.settle_deadline = Instant::now() + settle_for;
    }
}

pub fn coalesce_watch_events<I, P>(
    root_path: impl Into<PathBuf>,
    paths: I,
    settle_for: Duration,
) -> WatchEventBatch
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    let mut batch = WatchEventBatch::new(root_path, settle_for);
    for path in paths {
        batch.push_path(path, settle_for);
    }
    batch.full_reconciliation = batch.changed_paths.len() > 32;
    batch
}

pub struct LibraryWatcher {
    _watcher: notify::RecommendedWatcher,
    receiver: Receiver<notify::Result<notify::Event>>,
}

impl LibraryWatcher {
    pub fn start(root: impl AsRef<Path>) -> Result<Self, LibraryError> {
        let (sender, receiver) = unbounded();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .map_err(|err| LibraryError::Watch(err.to_string()))?;
        watcher
            .watch(root.as_ref(), RecursiveMode::Recursive)
            .map_err(|err| LibraryError::Watch(err.to_string()))?;
        Ok(Self {
            _watcher: watcher,
            receiver,
        })
    }

    pub fn try_recv(&self) -> Option<notify::Result<notify::Event>> {
        self.receiver.try_recv().ok()
    }
}
