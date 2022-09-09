use crate::rewrite::diff::FileOperation;
use crate::rewrite::fs_utils;
use crate::rewrite::gpg;
use anyhow::Result;
use std::path::Path;

fn dir_empty(dir: &Path) -> anyhow::Result<bool> {
    Ok(std::fs::read_dir(dir)?.next().is_none())
}

// TODO move to file_utils or gpg
pub fn encrypt_file(source: &Path, target: &Path, passphrase: &str) -> anyhow::Result<()> {
    let prefix = target.parent().unwrap();
    std::fs::create_dir_all(prefix).unwrap();
    println!(
        "encrypting {:?} and placing the result in {:?}",
        &source, &target
    );

    let mut plain_f = fs_utils::open_read(source)?;
    let mut enc_f = fs_utils::open_write(target)?;
    gpg::encrypt(&mut plain_f, &mut enc_f, passphrase.as_bytes())?;

    Ok(())
}

// TODO move to file_utils or gpg
pub fn decrypt_file(source: &Path, target: &Path, passphrase: &str) -> anyhow::Result<()> {
    let prefix = target.parent().unwrap();
    std::fs::create_dir_all(prefix)?;
    println!(
        "derypting {:?} and placing the result in {:?}",
        &source, &target
    );

    let mut enc_f = fs_utils::open_read(source)?;
    let mut plain_f = fs_utils::open_write(target)?;
    gpg::decrypt(&mut enc_f, &mut plain_f, passphrase.as_bytes())?;

    Ok(())
}

pub fn perform_file_ops(
    file_ops: &Vec<FileOperation>,
    plain_root: &Path,
    enc_root: &Path,
    passphrase: &str,
) -> anyhow::Result<()> {
    for fop in file_ops {
        // Make 100% sure that file ops can only touch files beneath the two dirs. I don't want to accidentally rm -rf / just because an empty absolute path snuck in there.
        match fop {
            FileOperation::DeleteEnc(pe) => {
                assert!(pe.is_relative());

                let target = enc_root.join(&pe);
                println!("deleting enc {:?}", &target);
                assert!(target.exists());
                if target.is_dir() {
                    std::fs::remove_dir_all(&target);
                } else if target.is_file() {
                    std::fs::remove_file(&target);
                } else {
                    panic!("neither dir nor file");
                }
                assert!(!target.exists());
            }
            FileOperation::DeletePlain(pp) => {
                assert!(pp.is_relative());

                let target = plain_root.join(&pp);
                println!("deleting plain {:?}", &target);
                assert!(target.exists());
                if target.is_dir() {
                    std::fs::remove_dir_all(&target);
                } else if target.is_file() {
                    std::fs::remove_file(&target);
                } else {
                    panic!("neither dir nor file");
                }
                assert!(!target.exists());
            }
            FileOperation::EncryptPlain(pp) => {
                assert!(pp.is_relative());

                let source = plain_root.join(&pp);
                assert!(source.exists());
                if source.is_dir() {
                    let target_dir = enc_root.join(&pp);
                    // assert!(!target.exists());
                    if target_dir.exists() {
                        println!("deleting target {:?}", &target_dir);
                        if target_dir.is_dir() {
                            std::fs::remove_dir_all(&target_dir);
                        } else {
                            std::fs::remove_file(&target_dir);
                        }
                    }
                    if dir_empty(&source)? {
                        // just create the corresponding directory
                        println!("creating {:?}", &target_dir);
                        std::fs::create_dir_all(&target_dir)?;
                    } else {
                        // recursively encrypt
                        crate::fileutils::visit_dir(&source, &mut |de: &std::fs::DirEntry| {
                            // visit_dir currently only yields files
                            assert!(de.path().is_file());
                            let target_file_path = fs_utils::add_gpg_suffix(
                                &enc_root.join(de.path().strip_prefix(&plain_root).unwrap()),
                            );
                            encrypt_file(&de.path(), &target_file_path, &passphrase)?;
                            Ok(())
                        });
                    }
                } else {
                    let target = enc_root.join(fs_utils::add_gpg_suffix(&pp));
                    //assert!(!target.exists());
                    if target.exists() {
                        // TODO delete it first
                        println!("overwriting target {:?}", &target);
                    }
                    encrypt_file(&source, &target, &passphrase)?;
                }
            }
            FileOperation::DecryptEnc(pe) => {
                assert!(pe.is_relative());

                let source = enc_root.join(&pe);
                assert!(source.exists());
                if source.is_dir() {
                    if dir_empty(&source)? {
                        let target = plain_root.join(&pe);
                        // assert!(!target.exists());
                        if target.exists() {
                            println!("deleting target {:?}", &target);
                            if target.is_dir() {
                                std::fs::remove_dir_all(&target);
                            } else {
                                std::fs::remove_file(&target);
                            }
                        }
                        // just create the corresponding directory
                        println!("creating {:?}", &target);
                        std::fs::create_dir_all(&target)?;
                    } else {
                        // recursively decrypt
                        crate::fileutils::visit_dir(&source, &mut |de: &std::fs::DirEntry| {
                            // visit_dir currently only yields files
                            assert!(de.path().is_file());
                            let target_file_path = fs_utils::remove_gpg_suffix(
                                &plain_root.join(de.path().strip_prefix(&enc_root).unwrap()),
                            );
                            decrypt_file(&de.path(), &target_file_path, &passphrase)?;
                            Ok(())
                        });
                    }
                } else {
                    let target = plain_root.join(fs_utils::remove_gpg_suffix(&pe));
                    // assert!(!target.exists());
                    if target.exists() {
                        println!("overwriting target {:?}", &target);
                    }
                    decrypt_file(&source, &target, &passphrase)?;
                }
            }
            FileOperation::ConflictCopyEnc(pe, pec) => {
                assert!(pe.is_relative());
                assert!(pec.is_relative());

                let source = enc_root.join(pe);
                let target = enc_root.join(pec);
                println!("copying enc {:?} to {:?}", &source, &target);

                if source.is_dir() {
                    // recursively decrypt
                    crate::fileutils::visit_dir(&source, &mut |de: &std::fs::DirEntry| {
                        // visit_dir currently only yields files
                        assert!(de.path().is_file());
                        let target_file_path =
                            &target.join(de.path().strip_prefix(&source).unwrap());
                        std::fs::copy(de.path(), target_file_path)?;

                        Ok(())
                    });
                } else {
                    if target.exists() {
                        println!("overwriting target {:?}", &target);
                        assert!(!target.is_dir())
                    }
                    std::fs::copy(source, target)?;
                }
            }
            FileOperation::ConflictCopyPlain(pp, ppc) => {
                assert!(pp.is_relative());
                assert!(ppc.is_relative());

                let source = plain_root.join(pp);
                let target = plain_root.join(ppc);

                println!("copying plain {:?} to {:?}", &source, &target);

                if source.is_dir() {
                    // recursively decrypt
                    crate::fileutils::visit_dir(&source, &mut |de: &std::fs::DirEntry| {
                        // visit_dir currently only yields files
                        assert!(de.path().is_file());
                        let target_file_path =
                            &target.join(de.path().strip_prefix(&source).unwrap());
                        std::fs::copy(de.path(), target_file_path)?;

                        Ok(())
                    });
                } else {
                    if target.exists() {
                        println!("overwriting target {:?}", &target);
                        assert!(!target.is_dir())
                    }
                    std::fs::copy(source, target)?;
                }
            }
        }
    }
    Ok(())
}

// TODO write tests

// target is overwritten by DecryptEnc EncryptPlain

// recursive conflictcopy creates parent directories if not present
