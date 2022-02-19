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
