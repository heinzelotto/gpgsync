use std::path::{Path, PathBuf};
use std::time::Duration;

/// Delay for which filesystem events are held back to e. g. clean up duplicates.
const WATCHER_DEBOUNCE_DURATION: Duration = Duration::from_secs(1);

pub struct Notifier {
    rx: std::sync::mpsc::Receiver<notify::DebouncedEvent>,
    _watcher: notify::RecommendedWatcher,
}

impl Notifier {
    fn new(root: &Path) -> anyhow::Result<Self> {
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

#[derive(Debug,Eq, PartialEq)]
enum Dirt {
    // Created,
    Modified,
    // Renamed(orig),
    Deleted,
}

// enum TreeNode {
//     Directory(Option<Dirt>, std::collections::HashMap<String, TreeNode>),
//     File(Option<Dirt>),
// }
#[derive(Debug)]
pub struct TreeNode(
    std::time::SystemTime,
    Option<Dirt>,
    std::collections::HashMap<String, TreeNode>,
);

impl TreeNode {
    fn clean(&mut self) {
        // match self {
        //     TreeNode::Directory(ref mut dirt, ref mut map) => {
        if self.1.is_some() {
            self.1 = None;
            for child in self.2.values_mut() {
                child.clean();
            }
        }
        //     }
        //     TreeNode::File(ref mut dirt) => {
        //         *dirt = None;
        //     }
        // }
    }

    fn dfs<F>(&mut self, fun: &mut F) where F: FnMut(&mut TreeNode) -> () {
            for nb in self.2.values_mut() {
                nb.dfs(fun);
            }

        fun(self);
    }
}

#[derive(Debug)]
pub struct Tree {
    root: TreeNode,
}

impl Tree {
    fn new() -> Self {
        Self {
            root: TreeNode(
                std::time::SystemTime::now(),
                None,
                std::collections::HashMap::new(),
            ),
        }
    }

    fn clean(&mut self) {
        self.root.clean();
    }

    fn write(&mut self, path: &Path, mtime: std::time::SystemTime) {
        // TODO recurse?

        let mut n = &mut self.root;

        for segment in path.iter() {
            // TODO OsStr
            n = &mut *n
                .2
                .entry(segment.to_string_lossy().to_string())
                .or_insert(TreeNode(
                    mtime,
                    Some(Dirt::Modified),
                    std::collections::HashMap::new(),
                ));

            n.0 = mtime;
            n.1 = Some(Dirt::Modified);
        }
    }

    // TODO not really an mtime
    fn delete(&mut self, path: &Path, mtime: std::time::SystemTime) {
        let mut n = &mut self.root;

        for segment in path.iter() {
            // TODO OsStr
            n = &mut *n
                .2
                .entry(segment.to_string_lossy().to_string())
                .or_insert(TreeNode(
                    mtime,
                    Some(Dirt::Modified),
                    std::collections::HashMap::new(),
                ));

            n.0 = mtime;
            n.1 = Some(Dirt::Modified);
        }

        n.dfs(&mut |cur: &mut TreeNode| {
            cur.0 = mtime;
            cur.1 = Some(Dirt::Deleted);
        });
    }

    fn rename(&mut self, path: &Path) {}
}

pub struct TreeReconciler {}

enum FileOperation {
    Deletion(PathBuf),
    Encryption(PathBuf),
    Decryption(PathBuf),
}

impl TreeReconciler {
    fn reconcile() -> Vec<FileOperation> {
        vec![]
    }
}

#[cfg(test)]
mod test {

    use super::{Dirt,Tree, TreeNode};

    use lazy_static::lazy_static;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    #[test]
    fn test_tree() {
        let mut tree = Tree::new();

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        tree.write(&Path::new("sub/dir/file.txt"), t0);

        dbg!(&tree);

        assert_eq!(tree.root.2.len(), 1);
        assert!(tree.root.2["sub"].2["dir"].2.contains_key("file.txt"));

        tree.delete(&Path::new("sub/dir"), t1);

        dbg!(&tree);

        assert_eq!(tree.root.2["sub"].2["dir"].1, Some(Dirt::Deleted));
        assert_eq!(tree.root.2["sub"].2["dir"].2["file.txt"].1, Some(Dirt::Deleted));
    }
}
