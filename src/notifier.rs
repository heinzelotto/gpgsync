#![allow(unused, dead_code)]

use std::os::unix::prelude::MetadataExt;
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

// TODO could also have treenodeEnc and treenodePlain

// TODO could also store just the filename without .gpg, and forbid a/b.txt/ and
// a/b.txt.gpg right at filesystem level (with panic) so that ambiguities never
// reach the tree.

#[derive(Debug, PartialEq, Clone)]
pub struct TreeNode {
    mtime: std::time::SystemTime, // TODO ?is this even needed
    dirt: Option<Dirt>,
    /// The keys are the path segment names as on the disk. I. e. for the
    /// encrypted tree there is a .gpg suffix on files.
    children: Option<std::collections::HashMap<String, TreeNode>>,
}

impl TreeNode {
    fn new_dir(
        mtime: std::time::SystemTime, // TODO ?is this even needed
        dirt: Option<Dirt>,
        children: std::collections::HashMap<String, TreeNode>,
    ) -> Self {
        TreeNode {
            mtime,
            dirt,
            children: Some(children),
        }
    }

    fn new_file(
        mtime: std::time::SystemTime, // TODO ?is this even needed
        dirt: Option<Dirt>,
    ) -> Self {
        TreeNode {
            mtime,
            dirt,
            children: None,
        }
    }

    fn clean(&mut self) {
        // match self {
        //     TreeNode::Directory(ref mut dirt, ref mut map) => {
        if self.dirt.is_some() {
            self.dirt = None;
            println!("cleaning"); // TODO remove

            if let Some(children) = &mut self.children {
                for k in children.keys() {
                    println!("cleaning node {}", k); // TODO remove
                }
                for child in children.values_mut() {
                    child.clean();
                }
            }
        }
        // TODO remove deleted subtrees, or better yet, introduce extra function
        // process_dirt(). and conflictcopy conflicting subtrees

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

        if let Some(children) = &mut self.children {
            for nb in children.values_mut() {
                nb.dfs_preorder(fun);
            }
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
            if let Some(children) = &self.children {
                for (nb_name, nb_item) in children.iter() {
                    relpath.push(nb_name);

                    nb_item.dfs_preorder_path_impl(fun, relpath);

                    relpath.pop();
                }
            }
        }
    }

    fn dfs_postorder<F>(&self, fun: &mut F)
    where
        F: FnMut(&TreeNode) -> (),
    {
        if let Some(children) = &self.children {
            for nb in children.values() {
                nb.dfs_postorder(fun);
            }
        }

        fun(self);
    }

    fn dfs_postorder_mut<F>(&mut self, fun: &mut F)
    where
        F: FnMut(&mut TreeNode) -> (),
    {
        if let Some(children) = &mut self.children {
            for nb in children.values_mut() {
                nb.dfs_postorder_mut(fun);
            }
        }

        fun(self);
    }

    fn get<'a>(&'a mut self, p: &Path) -> Option<&'a mut TreeNode> {
        let segments: Vec<String> = p
            .iter()
            .map(|p_elem| p_elem.to_string_lossy().to_string())
            .collect();
        let mut n = self;
        for i in 0..segments.len() {
            match &mut n.children {
                Some(children) => {
                    if !children.contains_key(&segments[i]) {
                        return None;
                    }

                    n = children.get_mut(&segments[i]).unwrap();
                }
                None => return None,
            }
        }

        Some(n)
    }

    fn get_parent_of<'a>(&'a mut self, p: &Path) -> Option<&'a mut TreeNode> {
        let segments: Vec<String> = p
            .iter()
            .map(|p_elem| p_elem.to_string_lossy().to_string())
            .collect();
        let mut n = self;
        for i in 0..segments.len() {
            // TODO the following is currently not beautiful

            match &mut n.children {
                Some(children) => {
                    if !children.contains_key(&segments[i]) {
                        return None;
                    }
                }
                None => return None,
            }

            if i == segments.len() - 1 {
                return Some(n);
            }

            match &mut n.children {
                Some(children) => {
                    n = children.get_mut(&segments[i]).unwrap();
                }
                None => return None,
            }
        }

        None
    }
}

fn add_gpg_suffix(p: &Path) -> PathBuf {
    let mut p = p.to_path_buf();
    let mut filename = p.file_name().unwrap().to_string_lossy().to_string();
    filename.push_str(".gpg");
    p.pop();
    p.push(filename);
    p
}
fn remove_gpg_suffix(p: &Path) -> PathBuf {
    let mut p = p.to_path_buf();
    let mut filename = p.file_name().unwrap().to_string_lossy().to_string();
    filename.truncate(filename.len() - 4);
    p.pop();
    p.push(filename);
    p
}

#[derive(Debug, PartialEq)]
pub struct Tree {
    root: TreeNode,
}

impl Tree {
    fn new() -> Self {
        Self {
            root: TreeNode::new_dir(
                std::time::SystemTime::now(),
                None,
                std::collections::HashMap::new(),
            ),
        }
    }

    fn with_time(time: &std::time::SystemTime) -> Self {
        Self {
            root: TreeNode::new_dir(time.clone(), None, std::collections::HashMap::new()),
        }
    }

    // TODO name clean_dirt
    fn clean(&mut self) {
        self.root.clean();
    }

    fn write(&mut self, path: &Path, is_dir: bool, mtime: std::time::SystemTime) {
        // TODO recurse?

        // TODO If is_dir is false, this will make a file at path, even if it
        // was a directory before. ?Is this intended.

        let mut n = &mut self.root;

        n.mtime = mtime;
        n.dirt = Some(Dirt::PathDirt);

        // TODO only change None dirt to PathDirt, and not Modified or Deleted
        // this way the filesystem diff with preorder traversal will work
        // correctly

        for segment in path.iter() {
            // TODO OsStr
            n = &mut *n
                .children
                .as_mut()
                // TODO ?should we allow this.
                .expect("Tried to write a subdir node where there was an existing file node")
                .entry(segment.to_string_lossy().to_string())
                .or_insert(TreeNode::new_dir(
                    mtime,
                    Some(Dirt::PathDirt),
                    std::collections::HashMap::new(),
                ));

            n.mtime = mtime;
            n.dirt = Some(Dirt::PathDirt);
        }

        if !is_dir {
            // assert!(n.children.is_none() || n.children.unwrap().is_empty());
            n.children = None;
        }

        n.dirt = Some(Dirt::Modified);
    }

    // ensure a path exists in the tree without setting any dirt
    // fn create_nodirt(&mut self, path: &Path, mtime: std::time::SystemTime) {
    //     let mut n = &mut self.root;

    //     n.mtime = mtime;

    //     for segment in path.iter() {
    //         // TODO OsStr
    //         n = &mut *n
    //             .children
    //             .entry(segment.to_string_lossy().to_string())
    //             .or_insert(TreeNode::new(mtime, None, std::collections::HashMap::new()));

    //         n.mtime = mtime;
    //     }
    // }

    // TODO not really an mtime
    fn mark_deleted(&mut self, path: &Path, mtime: std::time::SystemTime) {
        // The path must be present.
        let mut n = &mut self.root;

        n.mtime = mtime;
        n.dirt = Some(Dirt::PathDirt);

        // TODO only change None dirt to PathDirt, and not Modified or Deleted
        // this way the filesystem diff with preorder traversal will work
        // correctly

        for segment in path.iter() {
            // TODO OsStr
            n = &mut *n
                .children
                .as_mut()
                // TODO ?should we allow this.
                .expect("Tried to write a subdir node where there was an existing file node")
                .get_mut(&segment.to_string_lossy().to_string())
                .unwrap();

            n.mtime = mtime;
            n.dirt = Some(Dirt::PathDirt);
        }

        // n.dirt = Some(Dirt::Deleted);
        n.dfs_preorder(&mut |cur: &mut TreeNode| {
            cur.mtime = mtime;
            cur.dirt = Some(Dirt::Deleted);
        });
    }

    fn rename(&mut self, path: &Path) {}

    fn get<'a>(&'a mut self, p: &Path) -> Option<&'a mut TreeNode> {
        self.root.get(p)
    }

    /// returns the parent of `p`, but only if `p` is present
    fn get_parent_of<'a>(&'a mut self, p: &Path) -> Option<&'a mut TreeNode> {
        self.root.get_parent_of(p)
    }

    // fn diff(&mut self, other: &Tree) {}
}

/// When a filesystem notification comes, the current tree gets updated by
/// diffing it against the filesystem. Files and folders that appeared new in
/// the file system are added as nodes with the attribute "Modified" and tree
/// nodes whose corresponding file or folder does not exist get marked as
/// "Deleted"
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

#[derive(Clone, Copy)]
enum TreeType {
    Encrypted,
    Plain,
}

// TODO ?implement abstract tree-zip parallel iterator of two tree structures

// ?subtree_of_interest must be a directory, not a file

// TODO what should happen when we have both "/dir/" and "/dir.gpg", they are both represented the same
// we might have to differentiate between dir treenodes and file treenodes, maybe via putting the
// children hashmap into an enum TreenodeChildren::{File, Dir<hashmap>}
impl TreeReconciler {
    fn diff_from_filesystem_rec(
        fs_root: &Path,
        tr: &mut Tree,
        subtree_of_interest: &Path,
        tree_type: TreeType,
    ) -> std::io::Result<()> {
        let filesystem_corresponding_to_subtree = if fs_root.join(subtree_of_interest).exists() {
            Some(fs_root.join(&subtree_of_interest))
        } else {
            None
        };

        match (
            filesystem_corresponding_to_subtree, // TODO ?"subtree" should be "path", ?could be dir or file
            tr.get(&subtree_of_interest).is_some(),
        ) {
            (Some(fp), true) => {
                // TODO file <-> tree with children conflict case => clear tree children

                // TODO ?what if fs contains a file and tree a dir. check all the cases, write tests.

                // TODO filter .gpg (??and ignore upper_lowercase)
                let set_fs: std::collections::BTreeSet<String> =
                    std::fs::read_dir(fs_root.join(&fp))?
                        .map(|entry| {
                            entry.map(|ok_entry| ok_entry.file_name().to_string_lossy().to_string())
                        })
                        .collect::<Result<std::collections::BTreeSet<String>, std::io::Error>>()?;

                let set_tr: std::collections::BTreeSet<String> = tr
                    .get(&subtree_of_interest)
                    .unwrap()
                    .children
                    .as_mut()
                    .expect(
                        "TODO file <-> tree with children conflict case => ?clear tree children",
                    )
                    .keys()
                    .cloned()
                    .collect();

                for existing_child_name in set_fs.union(&set_tr) {
                    let child_on_fs = if fs_root.join(&existing_child_name).exists() {
                        Some(fs_root.join(&existing_child_name))
                    } else {
                        None
                    };

                    let mut recurse_necessary = true;

                    {
                        let tp = tr.get(&subtree_of_interest).unwrap();

                        let child_in_tr = tp
                            .children
                            .as_mut()
                            .expect("TODO currently must be dir")
                            .get(existing_child_name)
                            .map(|child| child.mtime);
                        if child_on_fs.is_some() && child_in_tr.is_some() {
                            let md = std::fs::metadata(fs_root.join(&fp))?;
                            let mtime_fs = md.modified()?;
                            if mtime_fs == child_in_tr.unwrap() {
                                recurse_necessary = false;
                            } else {
                                // TODO this depends on write() not overwriting existing dirt
                                tr.write(
                                    &subtree_of_interest.join(&existing_child_name),
                                    md.is_dir(),
                                    mtime_fs,
                                );
                            }
                        }
                    }

                    if recurse_necessary {
                        let child_dir = subtree_of_interest.join(existing_child_name);

                        TreeReconciler::diff_from_filesystem_rec(
                            fs_root, tr, &child_dir, tree_type,
                        )?;
                    }
                }

                // TODO implement
            }
            (Some(fp), false) => {
                let md = std::fs::metadata(&fp)?;
                tr.write(&subtree_of_interest, md.is_dir(), md.modified()?);

                if md.is_dir() {
                    for entry in std::fs::read_dir(&fp)? {
                        let entry = entry?;
                        let child_dir = subtree_of_interest.join(entry.file_name());

                        TreeReconciler::diff_from_filesystem_rec(
                            fs_root, tr, &child_dir, tree_type,
                        )?;
                    }
                }
            }
            (None, true) => {
                let top_mtime = tr.get(&subtree_of_interest).unwrap().mtime;

                // delete first manually to set pathdirt
                tr.mark_deleted(&subtree_of_interest, top_mtime);

                // TODO once pathdirts are only placed if there is no Dirt yet,
                // can also just call .delete on every node, but is log more
                // complex.

                tr.get(&subtree_of_interest)
                    .unwrap()
                    .dfs_postorder_mut(&mut |n: &mut TreeNode| {
                        n.dirt = Some(Dirt::Deleted);
                        n.mtime = top_mtime
                    });
            }
            (None, false) => {
                panic!("illegal diff case")
            }
        }

        Ok(())
    }

    /// TODO In the enc dir, files that dont have .gpg ending are ignored. In
    /// both enc and plain dir, directories that end in .gpg are ignored (for
    /// now). There should be helper methods to determine the validity of a fs
    /// object. The tree shall only contain valid things.
    fn diff_from_filesystem(
        fs_root: &Path,
        tr: &mut Tree,
        subtree_of_interest: &Path,
        tree_type: TreeType,
    ) -> std::io::Result<()> {
        // TODO strip .gpg from encrypted file names. Ignore non .gpg files in enc

        TreeReconciler::diff_from_filesystem_rec(fs_root, tr, subtree_of_interest, tree_type)?;

        Ok(())
    }
}

fn update_trees_with_changes(enc: &mut Tree, plain: &mut Tree, ops: &Vec<FileOperation>) {
    // TODO not in-place?
    for op in ops.iter() {
        match op {
            FileOperation::DeleteEnc(p) => {
                enc.root
                    .get_parent_of(&p)
                    .unwrap()
                    .children
                    .as_mut()
                    .unwrap()
                    .remove(&p.file_name().unwrap().to_string_lossy().to_string());
            }
            FileOperation::DeletePlain(p) => {
                plain
                    .root
                    .get_parent_of(&p)
                    .unwrap()
                    .children
                    .as_mut()
                    .unwrap()
                    .remove(&p.file_name().unwrap().to_string_lossy().to_string());
            }
            FileOperation::Encryption(p_plain) => {
                let target_node_clone = plain
                    .root
                    .get_parent_of(&p_plain)
                    .unwrap()
                    .children
                    .as_mut()
                    .unwrap()[&p_plain.file_name().unwrap().to_string_lossy().to_string()]
                    .clone();

                // add .gpg if it is a file
                let p_enc = if target_node_clone.children.is_some() {
                    p_plain.to_path_buf()
                } else {
                    add_gpg_suffix(&p_plain)
                };

                enc.write(
                    &p_enc,
                    target_node_clone.children.is_some(),
                    target_node_clone.mtime,
                );
                let encnode_parent = enc.root.get_parent_of(&p_enc).unwrap();

                encnode_parent.children.as_mut().unwrap().insert(
                    p_enc.file_name().unwrap().to_string_lossy().to_string(),
                    target_node_clone,
                );
            }
            FileOperation::Decryption(p_enc) => {
                let target_node_clone = enc
                    .root
                    .get_parent_of(&p_enc)
                    .unwrap()
                    .children
                    .as_mut()
                    .unwrap()[&p_enc.file_name().unwrap().to_string_lossy().to_string()]
                    .clone();

                // strip .gpg if it is a file
                let p_plain = if target_node_clone.children.is_some() {
                    p_enc.to_path_buf()
                } else {
                    remove_gpg_suffix(&p_enc)
                };

                plain.write(
                    &p_plain,
                    target_node_clone.children.is_some(),
                    target_node_clone.mtime,
                );
                let plainnode_parent = plain.root.get_parent_of(&p_plain).unwrap();

                plainnode_parent.children.as_mut().unwrap().insert(
                    p_plain.file_name().unwrap().to_string_lossy().to_string(),
                    target_node_clone,
                );
            }
            FileOperation::ConflictCopyEnc(p, q) => {
                let target_node_clone = enc
                    .root
                    .get_parent_of(&p)
                    .unwrap()
                    .children
                    .as_mut()
                    .unwrap()[&p.file_name().unwrap().to_string_lossy().to_string()]
                    .clone();

                enc.write(
                    &q,
                    target_node_clone.children.is_some(),
                    target_node_clone.mtime,
                );
                let qnode_parent = enc.root.get_parent_of(&q).unwrap();

                qnode_parent.children.as_mut().unwrap().insert(
                    q.file_name().unwrap().to_string_lossy().to_string(),
                    target_node_clone,
                );
            }
            FileOperation::ConflictCopyPlain(p, q) => {
                let target_node_clone = plain
                    .root
                    .get_parent_of(&p)
                    .unwrap()
                    .children
                    .as_mut()
                    .unwrap()[&p.file_name().unwrap().to_string_lossy().to_string()]
                    .clone();

                plain.write(
                    &q,
                    target_node_clone.children.is_some(),
                    target_node_clone.mtime,
                );
                let qnode_parent = plain.root.get_parent_of(&q).unwrap();

                qnode_parent.children.as_mut().unwrap().insert(
                    q.file_name().unwrap().to_string_lossy().to_string(),
                    target_node_clone,
                );
            }
        }
    }
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

        let curpath_enc = if cur.children.is_none() {
            add_gpg_suffix(&curpath)
        } else {
            curpath.clone()
        };

        let mut curpath_conflictcopy = other_side_deleted_root.map(|p: &PathBuf| {
            let mut curcopy = p.clone();
            curcopy.push(relpath);
            curcopy
        });

        match cur.dirt {
            Some(Dirt::Deleted) => {
                if other_side_deleted_root.is_none() {
                    ops.push(match tree_type {
                        TreeType::Encrypted => FileOperation::DeletePlain(curpath),
                        TreeType::Plain => FileOperation::DeleteEnc(curpath_enc),
                    });
                }

                // since we removed the whole subtree, don't recurse here
                return false;
            }
            Some(Dirt::PathDirt) => {}
            Some(Dirt::Modified) => {
                // let mut curpath = curpath.clone();

                ops.push(match (other_side_deleted_root.is_some(), &tree_type) {
                    (true, TreeType::Encrypted) => {
                        // enc tree already contains .gpg endings and so does the curpath_conflictcopy
                        FileOperation::ConflictCopyEnc(curpath, curpath_conflictcopy.unwrap())
                    }
                    (true, TreeType::Plain) => {
                        FileOperation::ConflictCopyPlain(curpath, curpath_conflictcopy.unwrap())
                    }
                    (false, TreeType::Encrypted) => FileOperation::Decryption(curpath_enc),
                    (false, TreeType::Plain) => FileOperation::Encryption(curpath),
                })
            }
            None => {}
        }
        return true;
    });
}

fn calculate_merge(enc: &Tree, plain: &Tree) -> Vec<FileOperation> {
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
    if enc.children.is_none() && plain.children.is_none() {
        // nothing to do
        return;
    } else if enc.children.is_none() ^ plain.children.is_none() {
        // TODO test case
        panic!("file <-> directory merge unsupported");
    }

    // we currently use a btreeset so that the ordering for the test is deterministic
    // let sete: std::collections::BTreeSet<String> =
    //     enc.children.as_ref().unwrap().keys().cloned().collect();
    // let setp: std::collections::BTreeSet<String> =
    //     plain.children.as_ref().unwrap().keys().cloned().collect();

    // normalized entry name (filenames in enc are stripped of the .gpg suffix) -> original enc entry name
    let mut subentries: std::collections::BTreeMap<String, Option<String>> =
        std::collections::BTreeMap::new();

    for (k, v) in enc.children.as_ref().unwrap() {
        if v.children.is_some() {
            // is a dir. Dirname could theoretically end in .gpg
            let entry = subentries.entry(k.clone()).or_insert(None);
            // same normalized entry name must not be set by two different enc entities (b.txt/ + b.txt.gpg)
            assert!(*entry == None);
            (*entry) = Some(k.clone());
        } else {
            // is a file. ignore files not ending in .gpg
            if k.ends_with(".gpg") {
                let non_gpg_filename = {
                    let mut x = k.clone();
                    x.truncate(k.len() - 4);
                    x
                };
                // same normalized entry name must not be set by two different enc entities (b.txt/ + b.txt.gpg)
                let entry = subentries.entry(non_gpg_filename).or_insert(None);
                assert!(*entry == None);
                (*entry) = Some(k.clone());
            }
        }
    }

    for (k, v) in plain.children.as_ref().unwrap() {
        subentries.entry(k.clone()).or_insert((None));
    }

    for (ke_normalized, original_ke_enc) in subentries {
        println!("current ke: {}", &ke_normalized);
        // retrieve possibly ke with added
        match dbg!((
            original_ke_enc.as_ref().and_then(|enc_entry| enc
                .children
                .as_ref()
                .unwrap()
                .get(enc_entry)),
            plain.children.as_ref().unwrap().get(&ke_normalized),
        )) {
            (Some(ne), Some(np)) => {
                let original_ke_enc = original_ke_enc.unwrap();

                let mut newpathe = curpath.clone();
                newpathe.push(&original_ke_enc);
                let mut newpathp = curpath.clone();
                newpathp.push(&ke_normalized);

                let mut newconflictcopypathe = curpath.clone();
                let copykee = format!(
                    "conflict_{}_{}",
                    ne.mtime
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    &original_ke_enc
                );
                newconflictcopypathe.push(copykee);

                let mut newconflictcopypathp = curpath.clone();
                let copykep = format!(
                    "conflict_{}_{}",
                    np.mtime
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    &ke_normalized
                );
                newconflictcopypathp.push(copykep);

                match dbg!((ne.dirt, np.dirt)) {
                    (None, None) => {}
                    (None, Some(_)) => {
                        println!("bla");
                        handle_independently(np, &newpathp, ops, TreeType::Plain, None);
                    }
                    (Some(_), None) => {
                        handle_independently(ne, &newpathe, ops, TreeType::Encrypted, None);
                    }
                    (Some(Dirt::Deleted), Some(Dirt::PathDirt)) => {
                        // recurse with the knowledge that the other side is to
                        // be deleted, i. e. conflictcopy this and delete the
                        // other
                        handle_independently(
                            np,
                            &newpathp,
                            ops,
                            TreeType::Plain,
                            Some(&newconflictcopypathp),
                        );
                        println!("ops1");
                        ops.push(FileOperation::DeletePlain(newpathp));
                    }
                    (Some(Dirt::PathDirt), Some(Dirt::Deleted)) => {
                        // recurse with the knowledge that the other side is to
                        // be deleted, i. e. conflictcopy this and delete the
                        // other
                        handle_independently(
                            ne,
                            &newpathe,
                            ops,
                            TreeType::Encrypted,
                            Some(&newconflictcopypathe),
                        );
                        println!("ops2");
                        ops.push(FileOperation::DeleteEnc(newpathe));
                    }
                    (Some(Dirt::Modified), Some(Dirt::PathDirt)) => {
                        // TODO assumption: can only be adding of this file/directory. changing of file attributes will not trigger this. also, adding a directory full of files will trigger modified dirt for all children.
                        // handle the pathdirt with a conflictcopy and apply the modification
                        handle_independently(
                            np,
                            &newpathp,
                            ops,
                            TreeType::Plain,
                            Some(&newconflictcopypathp),
                        );
                        ops.push(FileOperation::Decryption(newpathp));
                    }
                    (Some(Dirt::PathDirt), Some(Dirt::Modified)) => {
                        // handle the pathdirt with a conflictcopy and apply the modification
                        handle_independently(
                            ne,
                            &newpathe,
                            ops,
                            TreeType::Encrypted,
                            Some(&newconflictcopypathe),
                        );
                        ops.push(FileOperation::Encryption(newpathe));
                    }
                    (Some(Dirt::Modified), Some(Dirt::Modified)) => {
                        // conflictcopy plain, decrypt enc
                        ops.push(FileOperation::ConflictCopyPlain(
                            newpathp.clone(),
                            newconflictcopypathp,
                        ));
                        ops.push(FileOperation::Decryption(newpathe));
                    }
                    (Some(Dirt::Modified), Some(Dirt::Deleted)) => {
                        // conflictcopy the modified one and delete the original
                        // path, analog to the above
                        ops.push(FileOperation::ConflictCopyEnc(
                            PathBuf::from(&newpathe),
                            newconflictcopypathe,
                        ));
                        println!("ops3");
                        ops.push(FileOperation::DeleteEnc(PathBuf::from(&newpathe)));
                    }
                    (Some(Dirt::Deleted), Some(Dirt::Modified)) => {
                        // conflictcopy the modified one and delete the original
                        // path, analog to the above
                        ops.push(FileOperation::ConflictCopyPlain(
                            PathBuf::from(&newpathp),
                            newconflictcopypathp,
                        ));
                        println!("ops4");
                        ops.push(FileOperation::DeletePlain(PathBuf::from(&newpathp)));
                    }
                    (Some(Dirt::Deleted), Some(Dirt::Deleted)) => {
                        // nothing to be done
                    }
                    (Some(Dirt::PathDirt), Some(Dirt::PathDirt)) => {
                        // in this case we have a dir and ke_normalized == original_ke_enc
                        assert!(ke_normalized == original_ke_enc);
                        curpath.push(&ke_normalized);
                        calculate_merge_rec(
                            &enc.children
                                .as_ref()
                                .expect("PathDirt enc node must be directory")[&original_ke_enc],
                            &plain
                                .children
                                .as_ref()
                                .expect("PathDirt plain node must be directory")[&ke_normalized],
                            ops,
                            curpath,
                        );
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

    macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}

    use super::{
        calculate_merge, update_trees_with_changes, Dirt, FileOperation, Tree, TreeNode,
        TreeReconciler,
    };

    use lazy_static::lazy_static;
    use rand::Rng;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use std::{env, fs};

    #[test]
    fn test_tree() {
        let mut tree = Tree::new();

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        tree.write(&Path::new("sub/dir/file.txt"), false, t0);

        dbg!(&tree);
        assert_eq!(tree.root.children.as_ref().unwrap().len(), 1);
        assert!(tree.root.children.as_ref().unwrap()["sub"]
            .children
            .as_ref()
            .unwrap()["dir"]
            .children
            .as_ref()
            .unwrap()
            .contains_key("file.txt"));
        assert_eq!(
            tree.root.children.as_ref().unwrap()["sub"].dirt,
            Some(Dirt::PathDirt)
        );
        assert_eq!(
            tree.root.children.as_ref().unwrap()["sub"]
                .children
                .as_ref()
                .unwrap()["dir"]
                .dirt,
            Some(Dirt::PathDirt)
        );
        assert_eq!(
            tree.root.children.as_ref().unwrap()["sub"]
                .children
                .as_ref()
                .unwrap()["dir"]
                .children
                .as_ref()
                .unwrap()["file.txt"]
                .dirt,
            Some(Dirt::Modified)
        );

        tree.mark_deleted(&Path::new("sub/dir"), t1);

        dbg!(&tree);
        assert_eq!(
            tree.root.children.as_ref().unwrap()["sub"].dirt,
            Some(Dirt::PathDirt)
        );
        assert_eq!(
            tree.root.children.as_ref().unwrap()["sub"]
                .children
                .as_ref()
                .unwrap()["dir"]
                .dirt,
            Some(Dirt::Deleted)
        );
        assert_eq!(
            tree.root.children.as_ref().unwrap()["sub"]
                .children
                .as_ref()
                .unwrap()["dir"]
                .children
                .as_ref()
                .unwrap()["file.txt"]
                .dirt,
            Some(Dirt::Deleted)
        );
    }

    #[test]
    fn test_merge_1() -> anyhow::Result<()> {
        // del <-> del (top-level)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("f1.txt.gpg"), false, t0);
        tree_e.mark_deleted(&Path::new("f1.txt.gpg"), t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("f1.txt"), false, t1);
        tree_p.mark_deleted(&Path::new("f1.txt"), t1);
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
        tree_e.write(&Path::new("f1.txt.gpg"), false, t0);
        tree_e.clean();
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("f1.txt"), false, t1);
        tree_p.mark_deleted(&Path::new("f1.txt"), t1);
        dbg!(&tree_p);

        assert_eq!(
            calculate_merge(&tree_e, &tree_p),
            vec![FileOperation::DeleteEnc(PathBuf::from("f1.txt.gpg"))]
        );

        Ok(())
    }

    #[test]
    fn test_merge_3() -> anyhow::Result<()> {
        // mod <-> mod (top-level)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("f1.txt.gpg"), false, t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("f1.txt"), false, t1);
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
                FileOperation::Decryption(PathBuf::from("f1.txt.gpg"))
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
        tree_e.write(&Path::new("a/f1.txt.gpg"), false, t0);
        tree_e.write(&Path::new("a/f2.txt.gpg"), false, t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), false, t1);
        tree_p.write(&Path::new("a/f2.txt"), false, t1);
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
                FileOperation::Decryption(PathBuf::from("a/f1.txt.gpg")),
                FileOperation::ConflictCopyPlain(
                    PathBuf::from("a/f2.txt"),
                    PathBuf::from(format!(
                        "a/conflict_{}_f2.txt",
                        t1.duration_since(t0)?.as_secs()
                    ))
                ),
                FileOperation::Decryption(PathBuf::from("a/f2.txt.gpg")),
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
        tree_e.write(&Path::new("a/f1.txt.gpg"), false, t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), false, t1);
        tree_p.clean();
        tree_p.mark_deleted(&Path::new("a"), t1);
        dbg!(&tree_p);

        assert_eq!(
            calculate_merge(&tree_e, &tree_p),
            vec![
                FileOperation::ConflictCopyEnc(
                    PathBuf::from("a/f1.txt.gpg"),
                    PathBuf::from(format!(
                        "conflict_{}_a/f1.txt.gpg",
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
        tree_e.write(&Path::new("a/f1.txt.gpg"), false, t0);
        tree_e.clean();
        tree_e.mark_deleted(&Path::new("a"), t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), false, t1);
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

    #[test]
    fn test_merge_8() -> anyhow::Result<()> {
        // pathdirt/del <-> pathdirt/mod (conflicting on same levels)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("a/f1.txt.gpg"), false, t0);
        tree_e.mark_deleted(&Path::new("a/f1.txt.gpg"), t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), false, t1);
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
                FileOperation::DeletePlain(PathBuf::from("a/f1.txt")),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_merge_9() -> anyhow::Result<()> {
        // pathdirt/mod <-> pathdirt/del (conflicting on same levels)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("a/f1.txt.gpg"), false, t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), false, t1);
        tree_p.mark_deleted(&Path::new("a/f1.txt"), t1);
        dbg!(&tree_p);

        assert_eq!(
            calculate_merge(&tree_e, &tree_p),
            vec![
                FileOperation::ConflictCopyEnc(
                    PathBuf::from("a/f1.txt.gpg"),
                    PathBuf::from(format!(
                        "a/conflict_{}_f1.txt.gpg",
                        t0.duration_since(t0)?.as_secs()
                    ))
                ),
                FileOperation::DeleteEnc(PathBuf::from("a/f1.txt.gpg")),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_merge_10() -> anyhow::Result<()> {
        // mod <-> pathdirt/mod (conflicting on separate levels)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("a"), true, t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), false, t1);
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
                FileOperation::Decryption(PathBuf::from("a")),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_merge_11() -> anyhow::Result<()> {
        // mod <-> pathdirt/mod (conflicting on separate levels)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("a/f1.txt.gpg"), false, t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a"), true, t1);
        dbg!(&tree_p);

        assert_eq!(
            calculate_merge(&tree_e, &tree_p),
            vec![
                FileOperation::ConflictCopyEnc(
                    PathBuf::from("a/f1.txt.gpg"),
                    PathBuf::from(format!(
                        "conflict_{}_a/f1.txt.gpg",
                        t0.duration_since(t0)?.as_secs()
                    ))
                ),
                FileOperation::Encryption(PathBuf::from("a")),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_get_parent() -> anyhow::Result<()> {
        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree = Tree::new();
        tree.write(&Path::new("a/b/c/d/e/f1.txt"), false, t0);
        dbg!(&tree);

        assert_eq!(
            tree.root.get_parent_of(&Path::new("a")).cloned(),
            Some(tree.root.clone())
        );
        assert_eq!(
            tree.root.get_parent_of(&Path::new("a/b")).cloned(),
            Some(tree.root.children.as_ref().unwrap()["a"].clone())
        );
        assert_eq!(
            tree.root
                .get_parent_of(&Path::new("a/b/c/d/e/f1.txt"))
                .cloned(),
            Some(
                tree.root.children.as_ref().unwrap()["a"]
                    .children
                    .as_ref()
                    .unwrap()["b"]
                    .children
                    .as_ref()
                    .unwrap()["c"]
                    .children
                    .as_ref()
                    .unwrap()["d"]
                    .children
                    .as_ref()
                    .unwrap()["e"]
                    .clone()
            )
        );
        assert_eq!(tree.root.get_parent_of(&Path::new("")).cloned(), None);
        assert_eq!(tree.root.get_parent_of(&Path::new("xxx")).cloned(), None);
        assert_eq!(tree.root.get_parent_of(&Path::new("a/xxx")).cloned(), None);

        Ok(())
    }

    #[test]
    fn test_update_tree_0() -> anyhow::Result<()> {
        // ConflictCopyEnc (simple leaf in subdir)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let conflict_filename = format!("conflict_{}_f1.txt.gpg", t0.duration_since(t0)?.as_secs());
        let conflict_path = format!("a/conflict_{}_f1.txt.gpg", t0.duration_since(t0)?.as_secs());

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("a/f1.txt.gpg"), false, t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        dbg!(&tree_p);

        update_trees_with_changes(
            &mut tree_e,
            &mut tree_p,
            &vec![FileOperation::ConflictCopyEnc(
                PathBuf::from("a/f1.txt.gpg"),
                PathBuf::from(&conflict_path),
            )],
        );

        assert!(tree_e
            .root
            .get_parent_of(&Path::new(&conflict_path))
            .is_some());

        let tr = Tree {
            root: TreeNode::new_dir(
                t0,
                Some(Dirt::PathDirt),
                hashmap![String::from("a") => TreeNode::new_dir(
                    t0, Some(Dirt::PathDirt), hashmap![
                        conflict_filename.clone() => TreeNode::new_file(t0,Some(Dirt::Modified)),
                        String::from("f1.txt.gpg") => TreeNode::new_file(t0, Some(Dirt::Modified))
                    ])
                ],
            ),
        };

        // dbg!(&tree_e);
        // dbg!(&tr);
        assert_eq!(tree_e, tr);

        Ok(())
    }

    #[test]
    fn test_update_tree_1() -> anyhow::Result<()> {
        // ConflictCopyPlain (simple leaf in subdir)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let conflict_filename = format!("conflict_{}_f1.txt", t1.duration_since(t0)?.as_secs());
        let conflict_path = format!("a/conflict_{}_f1.txt", t1.duration_since(t0)?.as_secs());

        let mut tree_e = Tree::new();
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), false, t1);
        dbg!(&tree_p);

        update_trees_with_changes(
            &mut tree_e,
            &mut tree_p,
            &vec![FileOperation::ConflictCopyPlain(
                PathBuf::from("a/f1.txt"),
                PathBuf::from(&conflict_path),
            )],
        );

        assert!(tree_p
            .root
            .get_parent_of(&Path::new(&conflict_path))
            .is_some());

        let tr = Tree {
            root: TreeNode::new_dir(
                t1,
                Some(Dirt::PathDirt),
                hashmap![String::from("a") => TreeNode::new_dir(
                    t1, Some(Dirt::PathDirt), hashmap![
                        conflict_filename.clone() => TreeNode::new_file(t1,Some(Dirt::Modified)),
                        String::from("f1.txt") => TreeNode::new_file(t1, Some(Dirt::Modified))
                    ])
                ],
            ),
        };

        // dbg!(&tree_e);
        // dbg!(&tr);
        assert_eq!(tree_p, tr);

        Ok(())
    }

    #[test]
    fn test_update_tree_2() -> anyhow::Result<()> {
        // DeleteEnc (simple leaf in subdir)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("a/f1.txt.gpg"), false, t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        dbg!(&tree_p);

        update_trees_with_changes(
            &mut tree_e,
            &mut tree_p,
            &vec![FileOperation::DeleteEnc(PathBuf::from("a/f1.txt.gpg"))],
        );

        let tr = Tree {
            root: TreeNode::new_dir(
                t0,
                Some(Dirt::PathDirt),
                hashmap![String::from("a") => TreeNode::new_dir(
                    t0, Some(Dirt::PathDirt), hashmap![])
                ],
            ),
        };

        assert_eq!(tree_e, tr);

        Ok(())
    }

    #[test]
    fn test_update_tree_3() -> anyhow::Result<()> {
        // DeletePlain (simple leaf in subdir)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), false, t1);
        dbg!(&tree_p);

        update_trees_with_changes(
            &mut tree_e,
            &mut tree_p,
            &vec![FileOperation::DeletePlain(PathBuf::from("a/f1.txt"))],
        );

        let tr = Tree {
            root: TreeNode::new_dir(
                t1,
                Some(Dirt::PathDirt),
                hashmap![String::from("a") => TreeNode::new_dir(
                    t1, Some(Dirt::PathDirt), hashmap![]
                    )
                ],
            ),
        };

        assert_eq!(tree_p, tr);

        Ok(())
    }

    #[test]
    fn test_update_tree_4() -> anyhow::Result<()> {
        // Encryption (simple leaf in subdir)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        tree_p.write(&Path::new("a/f1.txt"), false, t1);
        dbg!(&tree_p);

        update_trees_with_changes(
            &mut tree_e,
            &mut tree_p,
            &vec![FileOperation::Encryption(PathBuf::from("a/f1.txt"))],
        );

        let tr = Tree {
            root: TreeNode::new_dir(
                t1,
                Some(Dirt::PathDirt),
                hashmap![String::from("a") => TreeNode::new_dir(
                    t1, Some(Dirt::PathDirt), hashmap![
                        String::from("f1.txt.gpg") => TreeNode::new_file(t1, Some(Dirt::Modified))
                    ])
                ],
            ),
        };

        assert_eq!(tree_e, tr);

        Ok(())
    }

    #[test]
    fn test_update_tree_5() -> anyhow::Result<()> {
        // Decryption (simple leaf in subdir)

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let mut tree_e = Tree::new();
        tree_e.write(&Path::new("a/f1.txt.gpg"), false, t0);
        dbg!(&tree_e);

        let mut tree_p = Tree::new();
        dbg!(&tree_p);

        update_trees_with_changes(
            &mut tree_e,
            &mut tree_p,
            &vec![FileOperation::Decryption(PathBuf::from("a/f1.txt.gpg"))],
        );

        let tr = Tree {
            root: TreeNode::new_dir(
                t0,
                Some(Dirt::PathDirt),
                hashmap![String::from("a") => TreeNode::new_dir(
                    t0, Some(Dirt::PathDirt), hashmap![
                        String::from("f1.txt") => TreeNode::new_file(t0, Some(Dirt::Modified))
                    ])
                ],
            ),
        };

        assert_eq!(dbg!(tree_p), dbg!(tr));

        Ok(())
    }

    // TODO conflictcopy more tests
    // TODO DeleteEnc/Plain non-leaf subdir

    // TODO test case where a directory is replaced by a file
    // TODO test case where a dir is deleted but somethin within it then readded

    // TODO if .gpg is added to files in enc dir, test pseude conflict of dir x and file x(.gpg)

    fn get_temp_dir() -> anyhow::Result<PathBuf> {
        let mut rng = rand::thread_rng();
        let mut dir = env::temp_dir();
        dir.push(format!("gpgsync_{}", rng.gen::<u32>()));

        fs::create_dir_all(&dir)?;

        Ok(dir)
    }

    fn make_file(p: &Path, s: &[u8]) -> anyhow::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(p)?;
        f.write_all(s)?;

        Ok(())
    }

    #[test]
    fn test_diff_from_filesystem_0() -> anyhow::Result<()> {
        // test (plain) file on filesystem <-> empty tree

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let fs_root = get_temp_dir()?;

        let mut f1 = fs_root.clone();
        f1.push("f1.txt");
        make_file(&f1, "test".as_bytes())?;
        let f1_mtime = std::fs::metadata(&f1)?.modified()?;

        let mut tr = Tree::with_time(&t0);

        let subtree_of_interest = Path::new("");

        TreeReconciler::diff_from_filesystem(
            &fs_root,
            &mut tr,
            &subtree_of_interest,
            super::TreeType::Plain,
        );

        std::fs::remove_dir_all(&fs_root);

        assert_eq!(
            tr,
            Tree {
                root: TreeNode::new_dir(
                    f1_mtime,
                    Some(Dirt::PathDirt),
                    hashmap![String::from("f1.txt") => TreeNode::new_file(
                        f1_mtime, Some(Dirt::Modified)
                    )]
                )
            }
        );

        Ok(())
    }

    #[test]
    fn test_diff_from_filesystem_1() -> anyhow::Result<()> {
        // test (plain) empty filesystem <-> tree with entry

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let fs_root = get_temp_dir()?;

        let mut tr = Tree::with_time(&t1);
        tr.write(&Path::new("f1.txt"), false, t1);
        tr.clean();

        let subtree_of_interest = Path::new("");

        TreeReconciler::diff_from_filesystem(
            &fs_root,
            &mut tr,
            &subtree_of_interest,
            super::TreeType::Plain,
        );

        std::fs::remove_dir_all(&fs_root);

        assert_eq!(
            tr,
            Tree {
                root: TreeNode::new_dir(
                    t1,
                    Some(Dirt::PathDirt),
                    hashmap![String::from("f1.txt") => TreeNode::new_file(
                        t1, Some(Dirt::Deleted)
                    )]
                )
            }
        );

        Ok(())
    }

    #[test]
    fn test_diff_from_filesystem_2() -> anyhow::Result<()> {
        // test (plain) file in filesystem <-> tree with older mtime

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let fs_root = get_temp_dir()?;
        let mut f1 = fs_root.clone();
        f1.push("f1.txt");
        make_file(&f1, "test".as_bytes())?;
        let f1_mtime = std::fs::metadata(&f1)?.modified()?;

        let mut tr = Tree::with_time(&t0);
        tr.write(&Path::new("f1.txt"), false, t0);
        tr.clean();

        let subtree_of_interest = Path::new("");

        TreeReconciler::diff_from_filesystem(
            &fs_root,
            &mut tr,
            &subtree_of_interest,
            super::TreeType::Plain,
        );

        std::fs::remove_dir_all(&fs_root);

        assert_eq!(
            tr,
            Tree {
                root: TreeNode::new_dir(
                    f1_mtime,
                    Some(Dirt::PathDirt),
                    hashmap![String::from("f1.txt") => TreeNode::new_file(
                        f1_mtime, Some(Dirt::Modified)
                    )]
                )
            }
        );

        Ok(())
    }

    #[test]
    fn test_diff_from_filesystem_3() -> anyhow::Result<()> {
        // test (plain) file in filesystem <-> tree with correct mtime

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let fs_root = get_temp_dir()?;
        let mut f1 = fs_root.clone();
        f1.push("f1.txt");
        make_file(&f1, "test".as_bytes())?;
        let f1_mtime = std::fs::metadata(&f1)?.modified()?;

        let mut tr = Tree::with_time(&t0);
        tr.write(&Path::new("f1.txt"), false, f1_mtime);
        tr.clean();

        let subtree_of_interest = Path::new("");

        TreeReconciler::diff_from_filesystem(
            &fs_root,
            &mut tr,
            &subtree_of_interest,
            super::TreeType::Plain,
        );

        std::fs::remove_dir_all(&fs_root);

        assert_eq!(
            tr,
            Tree {
                root: TreeNode::new_dir(
                    f1_mtime,
                    None,
                    hashmap![String::from("f1.txt") => TreeNode::new_file(
                        f1_mtime, None
                    )]
                )
            }
        );

        Ok(())
    }

    #[test]
    fn test_diff_from_filesystem_4() -> anyhow::Result<()> {
        // test (enc) file on filesystem <-> empty tree

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let fs_root = get_temp_dir()?;

        let mut f1 = fs_root.clone();
        f1.push("f1.txt.gpg");
        make_file(&f1, "test".as_bytes())?;
        let f1_mtime = std::fs::metadata(&f1)?.modified()?;

        let mut tr = Tree::with_time(&t0);

        let subtree_of_interest = Path::new("");

        TreeReconciler::diff_from_filesystem(
            &fs_root,
            &mut tr,
            &subtree_of_interest,
            super::TreeType::Encrypted,
        );

        std::fs::remove_dir_all(&fs_root);

        assert_eq!(
            tr,
            Tree {
                root: TreeNode::new_dir(
                    f1_mtime,
                    Some(Dirt::PathDirt),
                    hashmap![String::from("f1.txt.gpg") => TreeNode::new_file(
                        f1_mtime, Some(Dirt::Modified)
                    )]
                )
            }
        );

        Ok(())
    }

    #[test]
    fn test_diff_from_filesystem_5() -> anyhow::Result<()> {
        // test (enc) empty filesystem <-> tree with entry

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let fs_root = get_temp_dir()?;

        let mut tr = Tree::with_time(&t1);
        tr.write(&Path::new("f1.txt.gpg"), false, t1);
        tr.clean();

        let subtree_of_interest = Path::new("");

        TreeReconciler::diff_from_filesystem(
            &fs_root,
            &mut tr,
            &subtree_of_interest,
            super::TreeType::Encrypted,
        );

        std::fs::remove_dir_all(&fs_root);

        assert_eq!(
            tr,
            Tree {
                root: TreeNode::new_dir(
                    t1,
                    Some(Dirt::PathDirt),
                    hashmap![String::from("f1.txt.gpg") => TreeNode::new_file(
                        t1, Some(Dirt::Deleted)
                    )]
                )
            }
        );

        Ok(())
    }

    #[test]
    fn test_diff_from_filesystem_6() -> anyhow::Result<()> {
        // test (enc) file in filesystem <-> tree with older mtime

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let fs_root = get_temp_dir()?;
        let mut f1 = fs_root.clone();
        f1.push("f1.txt.gpg");
        make_file(&f1, "test".as_bytes())?;
        let f1_mtime = std::fs::metadata(&f1)?.modified()?;

        let mut tr = Tree::with_time(&t0);
        tr.write(&Path::new("f1.txt.gpg"), false, t0);
        tr.clean();

        let subtree_of_interest = Path::new("");

        TreeReconciler::diff_from_filesystem(
            &fs_root,
            &mut tr,
            &subtree_of_interest,
            super::TreeType::Encrypted,
        );

        std::fs::remove_dir_all(&fs_root);

        assert_eq!(
            tr,
            Tree {
                root: TreeNode::new_dir(
                    f1_mtime,
                    Some(Dirt::PathDirt),
                    hashmap![String::from("f1.txt.gpg") => TreeNode::new_file(
                        f1_mtime, Some(Dirt::Modified)
                    )]
                )
            }
        );

        Ok(())
    }

    #[test]
    fn test_diff_from_filesystem_7() -> anyhow::Result<()> {
        // test (enc) file in filesystem <-> tree with correct mtime

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let fs_root = get_temp_dir()?;
        let mut f1 = fs_root.clone();
        f1.push("f1.txt.gpg");
        make_file(&f1, "test".as_bytes())?;
        let f1_mtime = std::fs::metadata(&f1)?.modified()?;

        let mut tr = Tree::with_time(&t0);
        tr.write(&Path::new("f1.txt.gpg"), false, f1_mtime);
        tr.clean();

        let subtree_of_interest = Path::new("");

        TreeReconciler::diff_from_filesystem(
            &fs_root,
            &mut tr,
            &subtree_of_interest,
            super::TreeType::Encrypted,
        );

        std::fs::remove_dir_all(&fs_root);

        assert_eq!(
            tr,
            Tree {
                root: TreeNode::new_dir(
                    f1_mtime,
                    None,
                    hashmap![String::from("f1.txt.gpg") => TreeNode::new_file(
                        f1_mtime, None
                    )]
                )
            }
        );

        Ok(())
    }

    #[test]
    fn test_diff_from_filesystem_8() -> anyhow::Result<()> {
        // test (encrypted but without .gpg) file in filesystem ignored

        let t0 = std::time::SystemTime::UNIX_EPOCH;
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(1, 1);

        let fs_root = get_temp_dir()?;
        let mut f1 = fs_root.clone();
        f1.push("f1.txt.wrongextension");
        make_file(&f1, "test".as_bytes())?;
        let f1_mtime = std::fs::metadata(&f1)?.modified()?;

        let mut tr = Tree::with_time(&t0);

        let subtree_of_interest = Path::new("");

        TreeReconciler::diff_from_filesystem(
            &fs_root,
            &mut tr,
            &subtree_of_interest,
            super::TreeType::Encrypted,
        );

        std::fs::remove_dir_all(&fs_root);

        assert_eq!(
            tr,
            Tree {
                root: TreeNode::new_file(f1_mtime, None)
            }
        );

        Ok(())
    }

    // TODO more diff from filesystem for enc .gpg handling

    // TODO ignore non-.enc files in enc dir

    // TODO invalid subdir_of_interest, e. g. "." I think problems arise with
    // this because we interpret it as a file with name '.' and the fs just
    // ignores that path component

    // TODO created folder and created child file, ?pathdirt not overwritten

    // TODO files in subdir to test recursive diff

    // TODO helper function that can be parametrized to handle
    // filesystem/tree/result cases more easily.

    // TODO test where a/b.txt.gpg is deleted and then instantly after a/b.txt/
    // is created (i. e. it is file replaced by dir)
}
