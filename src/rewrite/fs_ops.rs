use crate::rewrite::diff::FileOperation;
use crate::rewrite::fs_utils;
use crate::rewrite::gpg;
use anyhow::Result;
use std::path::Path;

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
                    // recursively encrypt
                    unimplemented!();
                } else {
                    let target = enc_root.join(fs_utils::add_gpg_suffix(&pp));
                    assert!(!target.exists());
                    println!(
                        "encrypting {:?} and placing the result in {:?}",
                        &source, &target
                    );
                    let mut plain_f = fs_utils::open_read(&source)?;
                    let mut enc_f = fs_utils::open_write(&target)?;
                    gpg::encrypt(&mut plain_f, &mut enc_f, passphrase.as_bytes())?;
                }
            }
            FileOperation::DecryptEnc(pe) => {
                assert!(pe.is_relative());
                let source = enc_root.join(&pe);
                assert!(source.exists());
                if source.is_dir() {
                    // recursively decrypt
                    unimplemented!();
                } else {
                    let target = plain_root.join(fs_utils::remove_gpg_suffix(&pe));
                    assert!(!target.exists());
                    println!(
                        "derypting {:?} and placing the result in {:?}",
                        &source, &target
                    );
                    let mut enc_f = fs_utils::open_read(&source)?;
                    let mut plain_f = fs_utils::open_write(&target)?;
                    gpg::decrypt(&mut enc_f, &mut plain_f, passphrase.as_bytes())?;
                }
            }
            FileOperation::ConflictCopyEnc(pe, pec) => {
                assert!(pe.is_relative());
                assert!(pec.is_relative());
                println!(
                    "copying enc {:?} to {:?}",
                    enc_root.join(pe),
                    enc_root.join(pec)
                );
            }
            FileOperation::ConflictCopyPlain(pp, ppc) => {
                assert!(pp.is_relative());
                assert!(ppc.is_relative());
                println!(
                    "copying plain {:?} to {:?}",
                    plain_root.join(pp),
                    plain_root.join(ppc)
                );
            }
        }
    }
    Ok(())
}
