use std::os::unix::prelude::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::rewrite::diff::{FileOperation, TreeType};
use crate::rewrite::fs_utils::{add_gpg_suffix, remove_gpg_suffix};
use crate::rewrite::tree::{Dirt, Tree, TreeNode};

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

pub fn calculate_merge(enc: &Tree, plain: &Tree) -> Vec<FileOperation> {
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
        match (
            original_ke_enc
                .as_ref()
                .and_then(|enc_entry| enc.children.as_ref().unwrap().get(enc_entry)),
            plain.children.as_ref().unwrap().get(&ke_normalized),
        ) {
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

                match (ne.dirt, np.dirt) {
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

    use super::calculate_merge;
    use super::{Dirt, FileOperation, Tree, TreeNode};
    use std::path::{Path, PathBuf};

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
}
