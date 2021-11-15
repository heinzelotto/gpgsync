use std::path::{Path, PathBuf};
use std::time::Duration;

/// Delay for which filesystem events are held back to e. g. clean up duplicates.
const WATCHER_DEBOUNCE_DURATION: Duration = Duration::from_secs(1);

pub struct Notifier {
    rx: std::sync::mpsc::Receiver<notify::DebouncedEvent>,
    _watcher: notify::RecommendedWatcher,
}

impl Notifier {
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        use notify::Watcher;

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::watcher(tx, WATCHER_DEBOUNCE_DURATION)?;

        watcher.watch(&root, notify::RecursiveMode::Recursive)?;

        Ok(Self {
            rx,
            _watcher: watcher,
        })
    }
}
