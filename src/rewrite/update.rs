use super::fs_utils::{add_gpg_suffix, remove_gpg_suffix};
use crate::rewrite::diff::FileOperation;
use crate::rewrite::tree::{Dirt, Tree, TreeNode};

pub fn update_trees_with_changes(enc: &mut Tree, plain: &mut Tree, ops: &Vec<FileOperation>) {
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
#[cfg(test)]
mod test {

    macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}

    use super::{update_trees_with_changes, Dirt, FileOperation, Tree, TreeNode};

    use lazy_static::lazy_static;
    use rand::Rng;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use std::{env, fs};

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
}
