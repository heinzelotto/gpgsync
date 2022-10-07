use crate::rewrite::gpg;
use std::fs::File;
use std::path::{Path, PathBuf};

pub fn add_gpg_suffix(p: &Path) -> PathBuf {
    dbg!(&p);
    let mut p = p.to_path_buf();
    let mut filename = p.file_name().unwrap().to_string_lossy().to_string();
    filename.push_str(".gpg");
    p.pop();
    p.push(filename);
    p
}

pub fn remove_gpg_suffix(p: &Path) -> PathBuf {
    let mut p = p.to_path_buf();
    let mut filename = p.file_name().unwrap().to_string_lossy().to_string();
    assert!(&filename[filename.len() - 4..] == ".gpg");
    filename.truncate(filename.len() - 4);
    p.pop();
    p.push(filename);
    p
}

pub fn open_read(filename: &Path) -> std::io::Result<File> {
    File::open(filename)
}

pub fn open_write(filename: &Path) -> std::io::Result<File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(filename)
}

pub fn hash_all(p: &mut impl std::io::Read) -> anyhow::Result<Vec<u8>> {
    use sha1::Digest;

    let mut hasher = sha1::Sha1::new();

    let mut buf: [u8; 1024] = [0; 1024];
    loop {
        let n = p.read(&mut buf)?;
        if n == 0 {
            break;
        }

        hasher.update(&buf[0..n]);
    }

    Ok(hasher.finalize().to_vec())
}

pub fn plain_file_hash(p: &std::path::Path) -> anyhow::Result<Vec<u8>> {
    let mut f = open_read(p)?;

    hash_all(&mut f)
}

pub fn gpg_file_hash(p: &std::path::Path, passphrase: &str) -> anyhow::Result<Vec<u8>> {
    let mut f = open_read(p)?;

    let mut decrypted = Vec::new();

    gpg::decrypt(&mut f, &mut decrypted, passphrase.as_bytes())?;

    hash_all(&mut std::io::Cursor::new(decrypted))
}
