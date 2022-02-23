use notify::Watcher;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Delay for which filesystem events are held back to e. g. clean up duplicates.
const WATCHER_DEBOUNCE_DURATION: Duration = Duration::from_secs(1);

/// This RAII guard will resume watching of the `paused_path` on the `watcher` upon destruction
pub struct PauseGuard<'a> {
    paused_path: &'a Path,
    watcher: &'a mut notify::RecommendedWatcher,
}

impl<'a> PauseGuard<'a> {
    fn new(paused_path: &'a Path, watcher: &'a mut notify::RecommendedWatcher) -> Self {
        println!("Pausing notifications for {:?}", paused_path);
        watcher.unwatch(paused_path).unwrap();
        Self {
            paused_path,
            watcher,
        }
    }
}

impl<'a> std::ops::Drop for PauseGuard<'a> {
    fn drop(&mut self) {
        println!("Resuming notifications for {:?}", self.paused_path);
        self.watcher
            .watch(self.paused_path, notify::RecursiveMode::Recursive)
            .unwrap();
    }
}

pub struct Notifier {
    pub rx: crossbeam_channel::Receiver<notify::event::Event>,
    root: PathBuf,
    watcher: notify::RecommendedWatcher,
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
            root: root.to_owned(),
            watcher,
        })
    }

    /// Stop watching the root and restart watching once the returned guard goes out of scope.
    pub fn pause_watch<'a>(&'a mut self) -> PauseGuard<'a> {
        PauseGuard::new(&self.root, &mut self.watcher)
    }
}
