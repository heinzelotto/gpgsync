use std::path::{Path, PathBuf};
use std::time::Duration;

/// Delay for which filesystem events are held back to e. g. clean up duplicates.
const WATCHER_DEBOUNCE_DURATION: Duration = Duration::from_secs(1);

pub struct Notifier {
    pub rx: crossbeam_channel::Receiver<notify::event::Event>,
    _watcher: notify::RecommendedWatcher,
}

impl Notifier {
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        use notify::Watcher;

        let (tx, rx) = crossbeam_channel::unbounded();
        let mut watcher = notify::recommended_watcher(move |res| match res {
            Ok(event) => {
                tx.send(event);
            }
            Err(e) => panic!("watch error: {:?}", e),
        })?;

        watcher.watch(&root, notify::RecursiveMode::Recursive)?;

        Ok(Self {
            rx,
            _watcher: watcher,
        })
    }
}
