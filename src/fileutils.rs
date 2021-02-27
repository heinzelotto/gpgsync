use crate::filesync::FileStatus;
use std::fs::{DirEntry, File};
use std::path::{Path, PathBuf};

pub fn file_status(fp: &PathBuf) -> std::io::Result<FileStatus> {
    if !fp.exists() {
        return Ok(FileStatus::Nonexistent);
    }

    let mtime = std::fs::metadata(&fp)?.modified()?;
    Ok(FileStatus::Existent(mtime))
}

pub fn visit_dir(dir: &Path, cb: &mut dyn FnMut(&DirEntry)) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dir(&path, cb)?;
            } else {
                cb(&entry);
            }
        }
    }
    Ok(())
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
