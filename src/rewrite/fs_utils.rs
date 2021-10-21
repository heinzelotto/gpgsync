use std::path::{Path, PathBuf};

pub fn add_gpg_suffix(p: &Path) -> PathBuf {
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
    filename.truncate(filename.len() - 4);
    p.pop();
    p.push(filename);
    p
}
