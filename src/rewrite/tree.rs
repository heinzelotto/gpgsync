use std::os::unix::prelude::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum Dirt {
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
    pub mtime: std::time::SystemTime, // TODO ?is this even needed
    pub dirt: Option<Dirt>,
    pub hash: Option<Vec<u8>>,
    /// The keys are the path segment names as on the disk. I. e. for the
    /// encrypted tree there is a .gpg suffix on files.
    pub children: Option<std::collections::HashMap<String, TreeNode>>,
}

impl TreeNode {
    pub fn new_dir(
        mtime: std::time::SystemTime, // TODO ?is this even needed
        dirt: Option<Dirt>,
        hash: Option<Vec<u8>>,
        children: std::collections::HashMap<String, TreeNode>,
    ) -> Self {
        TreeNode {
            mtime,
            dirt,
            hash,
            children: Some(children),
        }
    }

    pub fn new_file(
        mtime: std::time::SystemTime, // TODO ?is this even needed
        dirt: Option<Dirt>,
        hash: Option<Vec<u8>>,
    ) -> Self {
        TreeNode {
            mtime,
            dirt,
            hash,
            children: None,
        }
    }

    pub fn is_dir(&self) -> bool {
        self.children.is_some()
    }

    pub fn is_file(&self) -> bool {
        self.children.is_none()
    }

    pub fn clean(&mut self) {
        // match self {
        //     TreeNode::Directory(ref mut dirt, ref mut map) => {
        if self.dirt.is_some() {
            self.dirt = None;

            if let Some(children) = &mut self.children {
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

    // pub fn dfs_preorder<F>(&mut self, fun: &mut F)
    // where
    //     F: FnMut(&mut TreeNode) -> (),
    // {
    //     fun(self);

    //     if let Some(children) = &mut self.children {
    //         for nb in children.values_mut() {
    //             nb.dfs_preorder(fun);
    //         }
    //     }
    // }

    pub fn dfs_preorder<F>(&mut self, fun: &mut F)
    where
        F: FnMut(&mut TreeNode) -> bool,
    {
        self.dfs_preorder_path_mut(&mut |tn, _| fun(tn));
    }

    pub fn dfs_preorder_path<F>(&self, fun: &mut F)
    where
        F: FnMut(&TreeNode, &Path) -> bool,
    {
        let mut relpath = PathBuf::new();
        self.dfs_preorder_path_impl(fun, &mut relpath);
    }

    pub fn dfs_preorder_path_impl<F>(&self, fun: &mut F, relpath: &mut PathBuf)
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

    pub fn dfs_preorder_path_mut<F>(&mut self, fun: &mut F)
    where
        F: FnMut(&mut TreeNode, &Path) -> bool,
    {
        let mut relpath = PathBuf::new();
        self.dfs_preorder_path_mut_impl(fun, &mut relpath);
    }

    pub fn dfs_preorder_path_mut_impl<F>(&mut self, fun: &mut F, relpath: &mut PathBuf)
    where
        F: FnMut(&mut TreeNode, &Path) -> bool,
    {
        if fun(self, &relpath) {
            if let Some(children) = &mut self.children {
                for (nb_name, nb_item) in children.iter_mut() {
                    relpath.push(nb_name);

                    nb_item.dfs_preorder_path_mut_impl(fun, relpath);

                    relpath.pop();
                }
            }
        }
    }

    pub fn dfs_postorder<F>(&self, fun: &mut F)
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

    pub fn dfs_postorder_mut<F>(&mut self, fun: &mut F)
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

    pub fn get<'a>(&'a mut self, p: &Path) -> Option<&'a mut TreeNode> {
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

    pub fn get_parent_of<'a>(&'a mut self, p: &Path) -> Option<&'a mut TreeNode> {
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

#[derive(Debug, PartialEq)]
pub struct Tree {
    pub root: TreeNode,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            root: TreeNode::new_dir(
                std::time::SystemTime::now(),
                None,
                None,
                std::collections::HashMap::new(),
            ),
        }
    }

    pub fn with_time(time: &std::time::SystemTime) -> Self {
        Self {
            root: TreeNode::new_dir(time.clone(), None, None, std::collections::HashMap::new()),
        }
    }

    // TODO name clean_dirt
    pub fn clean(&mut self) {
        self.root.clean();
    }

    pub fn write(&mut self, path: &Path, is_dir: bool, mtime: std::time::SystemTime) {
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
                    None,
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
    pub fn mark_deleted(&mut self, path: &Path, mtime: std::time::SystemTime) {
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

            true
        });
    }

    /// Go through all dirty paths, deleting whole subtrees that have Dirt::Deleted.
    pub fn prune_deleted(&mut self) {
        self.root.dfs_preorder(&mut |tn| {
            // We can only modify children but that is ok since we start at the root node which is never deleted.
            assert!(tn.dirt != Some(Dirt::Deleted));

            if let Some(ref mut hm) = &mut tn.children {
                hm.retain(|_, v| {
                    v.dirt != Some(Dirt::Deleted)});
            }

            match tn.dirt {
                Some(Dirt::Deleted) => {
                    panic!("Dirt::Deleted nodes must never be reached since they should have been deleted while traversing their parent step.");
                },
                Some(Dirt::Modified) => {
                    // TODO: confirm that if a directory contains modified + deleted, it will become pathdirt, thus we can skip this case
                   false
                },
                Some(Dirt::PathDirt) => {
                    // traverse
                    true
                },
                None => false,
            }
        })
    }

    fn rename(&mut self, path: &Path) {}

    pub fn get<'a>(&'a mut self, p: &Path) -> Option<&'a mut TreeNode> {
        self.root.get(p)
    }

    /// returns the parent of `p`, but only if `p` is present
    pub fn get_parent_of<'a>(&'a mut self, p: &Path) -> Option<&'a mut TreeNode> {
        self.root.get_parent_of(p)
    }

    // fn diff(&mut self, other: &Tree) {}
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

    use super::{Dirt, Tree, TreeNode};

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
}
