#![allow(unused, dead_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;
// #[allow(dead_code)]
// #![allow(unused)]
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

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
enum Dirt {
    // used to mark the path that will lead us to some real dirt
    PathDirt,
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
            println!("cleaning");

            for k in self.2.keys() {
                println!("cleaning node {}", k);
            }
            for child in self.2.values_mut() {
                child.clean();
            }
        }
        // TODO remove deleted subtrees, or better yet, introduce extra function process_dirt()

        //     }
        //     TreeNode::File(ref mut dirt) => {
        //         *dirt = None;
        //     }
        // }
    }

    fn dfs_preorder<F>(&mut self, fun: &mut F)
    where
        F: FnMut(&mut TreeNode) -> (),
    {
        fun(self);

        for nb in self.2.values_mut() {
            nb.dfs_preorder(fun);
        }
    }

    fn dfs_preorder_path<F>(&self, fun: &mut F)
    where
        F: FnMut(&TreeNode, &Path) -> bool,
    {
        let mut relpath = PathBuf::new();
        self.dfs_preorder_path_impl(fun, &mut relpath);
    }

    fn dfs_preorder_path_impl<F>(&self, fun: &mut F, relpath: &mut PathBuf)
    where
        F: FnMut(&TreeNode, &Path) -> bool,
    {
        if fun(self, &relpath) {
            for (nb_name, nb_item) in self.2.iter() {
                relpath.push(nb_name);

                nb_item.dfs_preorder_path_impl(fun, relpath);

                relpath.pop();
            }
        }
    }

    fn dfs_postorder<F>(&self, fun: &mut F)
    where
        F: FnMut(&TreeNode) -> (),
    {
        for nb in self.2.values() {
            nb.dfs_postorder(fun);
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

    // TODO name clean_dirt
    fn clean(&mut self) {
        self.root.clean();
    }

    // TODO we need

    fn write(&mut self, path: &Path, mtime: std::time::SystemTime) {
        // TODO recurse?

        let mut n = &mut self.root;

        n.0 = mtime;
        n.1 = Some(Dirt::PathDirt);

        for segment in path.iter() {
            // TODO OsStr
            n = &mut *n
                .2
                .entry(segment.to_string_lossy().to_string())
                .or_insert(TreeNode(
                    mtime,
                    Some(Dirt::PathDirt),
                    std::collections::HashMap::new(),
                ));

            n.0 = mtime;
            n.1 = Some(Dirt::PathDirt);
        }

        n.1 = Some(Dirt::Modified);
    }

    // TODO not really an mtime
    fn delete(&mut self, path: &Path, mtime: std::time::SystemTime) {
        let mut n = &mut self.root;

        n.0 = mtime;
        n.1 = Some(Dirt::PathDirt);

        for segment in path.iter() {
            // TODO OsStr
            n = &mut *n
                .2
                .entry(segment.to_string_lossy().to_string())
                .or_insert(TreeNode(
                    mtime,
                    Some(Dirt::PathDirt),
                    std::collections::HashMap::new(),
                ));

            n.0 = mtime;
            n.1 = Some(Dirt::PathDirt);
        }

        n.dfs_preorder(&mut |cur: &mut TreeNode| {
            cur.0 = mtime;
            cur.1 = Some(Dirt::Deleted);
        });
    }

    fn rename(&mut self, path: &Path) {}

    // fn diff(&mut self, other: &Tree) {}
}

pub struct TreeReconciler {}

#[derive(Debug, Eq, PartialEq)]
enum FileOperation {
    DeleteEnc(PathBuf),
    DeletePlain(PathBuf),
    Encryption(PathBuf),
    Decryption(PathBuf),
    ConflictCopyEnc(PathBuf, PathBuf),
    ConflictCopyPlain(PathBuf, PathBuf), // TODO could be a move but ?how to handle the rename or delete/modify notification from the notifier then
}

impl TreeReconciler {
    fn diff_from_filesystem(t: &mut Tree, subtree_of_interest: &Path) {}
}

enum TreeType {
    Encrypted,
    Plain,
}
fn handle_independently(
    ne: &TreeNode,
    root: &PathBuf,
    ops: &mut Vec<FileOperation>,
    tree_type: TreeType,
    other_side_deleted_root: Option<&PathBuf>,
) {
    ne.dfs_preorder_path(&mut |cur: &TreeNode, relpath: &Path| {
        let mut curpath = root.clone();
        curpath.push(relpath);

        let mut curpath_conflictcopy = other_side_deleted_root.map(|p: &PathBuf| {
            let mut curcopy = p.clone();
            curcopy.push(relpath);
            curcopy
        });

        match cur.1 {
            // TODO actually we don't need to delete every file
            // individually, we can just remove whole subtrees, make
            // this func return bool that states whether to continue or
            // break the traversal
            Some(Dirt::Deleted) => {
                if !other_side_deleted_root.is_some() {
                    ops.push(match tree_type {
                        TreeType::Encrypted => FileOperation::DeletePlain(curpath),
                        TreeType::Plain => FileOperation::DeleteEnc(curpath),
                    });
                }
                return false;
            }
            Some(Dirt::PathDirt) => {}
            Some(Dirt::Modified) => {
                let mut curpath = curpath.clone();

                ops.push(match (other_side_deleted_root.is_some(), &tree_type) {
                    (true, TreeType::Encrypted) => {
                        FileOperation::ConflictCopyEnc(curpath, curpath_conflictcopy.unwrap())
                    }
                    (true, TreeType::Plain) => {
                        FileOperation::ConflictCopyPlain(curpath, curpath_conflictcopy.unwrap())
                    }
                    (false, TreeType::Encrypted) => FileOperation::Decryption(curpath),
                    (false, TreeType::Plain) => FileOperation::Encryption(curpath),
                })
            }
            None => {}
        }
        return true;
    });
}

fn calculate_merge(enc: &Tree, plain: &Tree) -> Vec<FileOperation> {
    // if a subtree is dirty in both trees, conflictcopy operations on all
    // dirty files (always e. g. on the remote/enc tree, i. e. local
    // modification always wins) within the conflicting subtree.
    //
    // A file is a leaf TreeNode.
    //
    // ConflictCopy Operations need to add the date to the filename in case
    // more than one instance of gpgsync conflicts at the same time. (or
    // else we will have a.txt, a.conflict.txt, a.conflict.conflict.txt
    // after three iterations, ?which should also be fine)
    //
    // It is now the task to determine what to copy on which directory
    // level.
    //
    // both delete: no conflictcopy
    //
    // tree1/mod/mod/del/del, tree2/mod/mod/mod/mod: create a conflictcopy
    // of the topmost del/, then of all

    // if a subtree is dirty in at most one tree then resolve this subtree and its mirror without conflict.

    // conflictcopy operations should be performed first, ?but possibly after rename ops

    let mut ops = vec![];

    let mut path = PathBuf::new();
    calculate_merge_rec(&enc.root, &plain.root, &mut ops, &mut path);

    // TODO sort conflictcopy operations at front
    ops
}

fn calculate_merge_rec(
    enc: &TreeNode,
    plain: &TreeNode,
    ops: &mut Vec<FileOperation>,
    curpath: &mut PathBuf,
) {
    // we currently use a btreeset so that the ordering for the test is deterministic
    let sete: std::collections::BTreeSet<String> = enc.2.keys().cloned().collect();
    let setp: std::collections::BTreeSet<String> = plain.2.keys().cloned().collect();

    for ke in sete.union(&setp) {
        println!("{}", &ke);
        match (enc.2.get(ke), plain.2.get(ke)) {
            (Some(ne), Some(np)) => {
                let mut newpath = curpath.clone();
                newpath.push(ke);

                let mut newconflictcopypathe = curpath.clone();
                let copykee = format!(
                    "conflict_{}_{}",
                    ne.0.duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    ke
                );
                newconflictcopypathe.push(copykee);

                let mut newconflictcopypathp = curpath.clone();
                let copykep = format!(
                    "conflict_{}_{}",
                    np.0.duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    ke
                );
                newconflictcopypathp.push(copykep);

                match dbg!((ne.1, np.1)) {
                    (None, None) => {}
                    (None, Some(_)) => {
                        handle_independently(np, &newpath, ops, TreeType::Plain, None);
                    }
                    (Some(_), None) => {
                        handle_independently(ne, &newpath, ops, TreeType::Encrypted, None);
                    }
                    (Some(Dirt::Deleted), Some(Dirt::PathDirt)) => {
                        handle_independently(
                            np,
                            &newpath,
                            ops,
                            TreeType::Plain,
                            Some(&newconflictcopypathp),
                        );

                        ops.push(FileOperation::DeletePlain(PathBuf::from(ke)));
                    }
                    (Some(Dirt::PathDirt), Some(Dirt::Deleted)) => {
                        // TODO recurse with the knowledge that the other side is to be deleted, i. e. conflict
                        // then DeletePlain np if only the pathdirt on ne only led to other deletions and no modifications
                        //
                        // or: have all modifications on ne happen in an alternate conflictcopy path and handle independently

                        handle_independently(
                            ne,
                            &newpath,
                            ops,
                            TreeType::Encrypted,
                            Some(&newconflictcopypathe),
                        );

                        ops.push(FileOperation::DeleteEnc(PathBuf::from(ke)));
                    }
                    (Some(Dirt::Modified), Some(Dirt::PathDirt)) => { // TODO analog to the above
                    }
                    (Some(Dirt::PathDirt), Some(Dirt::Modified)) => { // TODO analog to the above
                    }
                    (Some(Dirt::Modified), Some(Dirt::Modified)) => {
                        // conflictcopy plain, decrypt enc
                        ops.push(FileOperation::ConflictCopyPlain(
                            newpath.clone(),
                            newconflictcopypathp,
                        ));
                        ops.push(FileOperation::Decryption(newpath));
                    }
                    (Some(Dirt::Modified), Some(Dirt::Deleted)) => {
                        // TODO conflictcopy the modified one and delete the original path, analog to the above
                    }
                    (Some(Dirt::Deleted), Some(Dirt::Modified)) => {
                        // TODO conflictcopy the modified one and delete the original path, analog to the above
                    }
                    (Some(Dirt::Deleted), Some(Dirt::Deleted)) => {
                        // nothing to be done
                    }
                    (Some(Dirt::PathDirt), Some(Dirt::PathDirt)) => {
                        curpath.push(&ke);
                        calculate_merge_rec(&enc.2[ke], &plain.2[ke], ops, curpath);
                        curpath.pop();
                    }
                }
            }
            (None, Some(np)) => {
                handle_independently(np, &curpath, ops, TreeType::Plain, None);
            }
            (Some(ne), None) => {
                handle_independently(ne, &curpath, ops, TreeType::Encrypted, None);
            }
            (None, None) => {
                panic!("illegal, one should have been present");
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod test {

    use super::{calculate_merge, Dirt, FileOperation, Tree, TreeNode};

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
        assert_eq!(tree.root.2["sub"].1, Some(Dirt::PathDirt));
        assert_eq!(tree.root.2["sub"].2["dir"].1, Some(Dirt::PathDirt));
        assert_eq!(
            tree.root.2["sub"].2["dir"].2["file.txt"].1,
            Some(Dirt::Modified)
        );

        tree.delete(&Path::new("sub/dir"), t1);

        dbg!(&tree);
        assert_eq!(tree.root.2["sub"].1, Some(Dirt::PathDirt));
        assert_eq!(tree.root.2["sub"].2["dir"].1, Some(Dirt::Deleted));
        assert_eq!(
            tree.root.2["sub"].2["dir"].2["file.txt"].1,
            Some(Dirt::Deleted)
        );
    }

    #[test]
    fn test_merge_1() -> anyhow::Result<()> {
        // del <-> del (top-level)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.delete(&Path::new("f1.txt"), t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.delete(&Path::new("f1.txt"), t1);
        dbg!(&tree_p);

        assert_eq!(calculate_merge(&tree_e, &tree_p), vec![]);

        Ok(())
    }

    #[test]
    fn test_merge_2() -> anyhow::Result<()> {
        // clean <-> del (top-level)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("f1.txt"), t0);
        tree_e.clean();
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.delete(&Path::new("f1.txt"), t1);
        dbg!(&tree_p);

        assert_eq!(
            calculate_merge(&tree_e, &tree_p),
            vec![FileOperation::DeleteEnc(PathBuf::from("f1.txt"))]
        );

        Ok(())
    }

    #[test]
    fn test_merge_3() -> anyhow::Result<()> {
        // mod <-> mod (top-level)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("f1.txt"), t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("f1.txt"), t1);
        dbg!(&tree_p);

        assert_eq!(
            calculate_merge(&tree_e, &tree_p),
            vec![
                FileOperation::ConflictCopyPlain(
                    PathBuf::from("f1.txt"),
                    PathBuf::from(format!(
                        "conflict_{}_f1.txt",
                        t1.duration_since(t0)?.as_secs()
                    ))
                ),
                FileOperation::Decryption(PathBuf::from("f1.txt"))
            ]
        );

        Ok(())
    }

    // #[test]
    //     fn test_merge_4() -> anyhow::Result<()> {
    //         let t0 = std::time::SystemTime::UNIX_EPOCH;
    //         let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

    //         let mut tree_e = Tree::new();
    //         tree_e.write(&Path::new("a/f1.txt"), t0);
    //         dbg!(&tree_e);

    //         let mut tree_p = Tree::new();
    //         tree_p.write(&Path::new("a/f1.txt"), t1);
    //         dbg!(&tree_p);

    //         assert_eq!(
    //             calculate_merge(&tree_e, &tree_p),
    //             vec![
    //                 FileOperation::ConflictCopyPlain(
    //                     PathBuf::from("a/f1.txt"),
    //                     PathBuf::from(format!(
    //                         "a/conflict_{}_f1.txt",
    //                         t0.duration_since(t0)?.as_secs()
    //                     ))
    //                 ),
    //                 FileOperation::Decryption(PathBuf::from("a/f1.txt"))
    //             ]
    //         );

    //         Ok(())
    //     }

    #[test]
    fn test_merge_5() -> anyhow::Result<()> {
        // mod <-> mod (conflicting mod within same subdir)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("a/f1.txt"), t0);
        tree_e.write(&Path::new("a/f2.txt"), t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), t1);
        tree_p.write(&Path::new("a/f2.txt"), t1);
        dbg!(&tree_p);

        assert_eq!(
            calculate_merge(&tree_e, &tree_p),
            vec![
                FileOperation::ConflictCopyPlain(
                    PathBuf::from("a/f1.txt"),
                    PathBuf::from(format!(
                        "a/conflict_{}_f1.txt",
                        t1.duration_since(t0)?.as_secs()
                    ))
                ),
                FileOperation::Decryption(PathBuf::from("a/f1.txt")),
                FileOperation::ConflictCopyPlain(
                    PathBuf::from("a/f2.txt"),
                    PathBuf::from(format!(
                        "a/conflict_{}_f2.txt",
                        t1.duration_since(t0)?.as_secs()
                    ))
                ),
                FileOperation::Decryption(PathBuf::from("a/f2.txt")),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_merge_6() -> anyhow::Result<()> {
        // pathdirt/mod <-> delete/ (conflicting on separate levels)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("a/f1.txt"), t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), t1);
        tree_p.clean();
        tree_p.delete(&Path::new("a"), t1);
        dbg!(&tree_p);

        assert_eq!(
            calculate_merge(&tree_e, &tree_p),
            vec![
                FileOperation::ConflictCopyEnc(
                    PathBuf::from("a/f1.txt"),
                    PathBuf::from(format!(
                        "conflict_{}_a/f1.txt",
                        t0.duration_since(t0)?.as_secs()
                    ))
                ),
                FileOperation::DeleteEnc(PathBuf::from("a")),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_merge_7() -> anyhow::Result<()> {
        // delete/ <-> pathdirt/mod (conflicting on separate levels)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("a/f1.txt"), t0);
        tree_e.clean();
        tree_e.delete(&Path::new("a"), t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), t1);
        dbg!(&tree_p);

        assert_eq!(
            calculate_merge(&tree_e, &tree_p),
            vec![
                FileOperation::ConflictCopyPlain(
                    PathBuf::from("a/f1.txt"),
                    PathBuf::from(format!(
                        "conflict_{}_a/f1.txt",
                        t1.duration_since(t0)?.as_secs()
                    ))
                ),
                FileOperation::DeletePlain(PathBuf::from("a")),
            ]
        );

        Ok(())
    }

    // TODO test case where a directory is replaced by a file
    // TODO test case where a dir is deleted but somethin within it then readded
}
