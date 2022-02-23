use std::path::{Path, PathBuf};

use crate::rewrite::tree::{Dirt, Tree, TreeNode};

/// When a filesystem notification comes, the current tree gets updated by
/// diffing it against the filesystem. Files and folders that appeared new in
/// the file system are added as nodes with the attribute "Modified" and tree
/// nodes whose corresponding file or folder does not exist get marked as
/// "Deleted"
pub struct TreeReconciler {}

/// Denotes an operation on one or both file system trees. Paths must always be relative to make sure that nothing outside of plain_root and gpg_root is touched.
#[derive(Debug, Eq, PartialEq)]
pub enum FileOperation {
    /// Delete the file or folder within the enc_root
    DeleteEnc(PathBuf),
    /// Delete the file or folder within the plain_root
    DeletePlain(PathBuf),
    /// Encrypt the file or folder in the plain_root and place the result in the enc_root.
    EncryptPlain(PathBuf),
    /// Decrypt the file or folder in the enc_root and place the result in the plain_root.
    DecryptEnc(PathBuf),
    /// Create a conflict copy from the file within enc_root to a renamed path within enc_root
    ConflictCopyEnc(PathBuf, PathBuf),
    /// Create a conflict copy from the file within plain_root to a renamed path within plain_root
    ConflictCopyPlain(PathBuf, PathBuf), // TODO could be a move but ?how to handle the rename or delete/modify notification from the notifier then
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum TreeType {
    Encrypted,
    Plain,
}

// TODO ?implement abstract tree-zip parallel iterator of two tree structures

// TODO ?subtree_of_interest must be a directory, not a file. Not sure

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
                // TODO currently seems like there subtree_of_interest should be a dir
                let set_fs: std::collections::BTreeSet<String> =
                    std::fs::read_dir(fs_root.join(&fp))?
                        // .filter(|entry| {
                        //     entry.as_ref().map(
                        //         |ok_entry| ok_entry.metadata().map(|md| !md.is_dir())
                        //     ).unwrap_or(Ok(true)).unwrap_or(true) && ok_entry.map(file)
                        // })
                        .filter(|entry| {
                            tree_type == TreeType::Plain
                                || entry.as_ref().map_or(true, |ok_entry| {
                                    !(ok_entry.metadata().map_or(true, |md| !md.is_dir())
                                        && ok_entry
                                            .path()
                                            .extension()
                                            .map_or(true, |ext| !ext.eq("gpg")))
                                })
                        })
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
    pub fn diff_from_filesystem(
        fs_root: &Path,
        tr: &mut Tree,
        subtree_of_interest: &Path,
        tree_type: TreeType,
    ) -> std::io::Result<()> {
        // TODO Ignore non .gpg files in enc

        TreeReconciler::diff_from_filesystem_rec(fs_root, tr, subtree_of_interest, tree_type)?;

        Ok(())
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

    use super::{Dirt, FileOperation, Tree, TreeNode, TreeReconciler};

    use lazy_static::lazy_static;
    use rand::Rng;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use std::{env, fs};

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

    fn make_dir(p: &Path, s: &[u8]) -> anyhow::Result<()> {
        fs::create_dir_all(&p)?;

        Ok(())
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
            dbg!(tr),
            dbg!(Tree {
                root: TreeNode::new_dir(
                    f1_mtime,
                    Some(Dirt::PathDirt),
                    hashmap![String::from("f1.txt") => TreeNode::new_file(
                        f1_mtime, Some(Dirt::Modified)
                    )]
                )
            })
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
            dbg!(tr),
            dbg!(Tree {
                root: TreeNode::new_dir(t0, None, hashmap![])
            })
        );

        Ok(())
    }
}
