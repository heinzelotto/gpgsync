use std::path::{Path, PathBuf};

/// A sync entity represents up to two files by a relative path. It can exist unencrypted relative to the plain_root and
/// encrypted (with .gpg extension) relative to the gpg_root.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct SyncEntity<'a> {
    rel_path_without_gpg: PathBuf,
    plain_root: &'a PathBuf,
    gpg_root: &'a PathBuf,
}

fn add_gpg_extension(p: &PathBuf) -> PathBuf {
    let mut name = p.file_name().unwrap().to_owned();
    name.push(".gpg");
    p.parent().unwrap().join(&name)
}

fn remove_gpg_extension(p: &Path) -> PathBuf {
    p.parent()
        .unwrap()
        .join(p.file_stem().unwrap())
        .to_path_buf()
}

impl<'a> SyncEntity<'a> {
    pub fn from_plain(
        plain_path: &PathBuf,
        plain_root: &'a PathBuf,
        gpg_root: &'a PathBuf,
    ) -> Self {
        let relative_path_without_gpg = plain_path
            .strip_prefix(&plain_root) // check that it is indeed a subpath
            .unwrap()
            .to_path_buf();
        Self {
            rel_path_without_gpg: relative_path_without_gpg,
            plain_root,
            gpg_root,
        }
    }
    pub fn from_gpg(gpg_path: &PathBuf, plain_root: &'a PathBuf, gpg_root: &'a PathBuf) -> Self {
        let rel_path_without_gpg = remove_gpg_extension(&gpg_path)
            .strip_prefix(&gpg_root) // check that it is indeed a subpath
            .unwrap()
            .to_path_buf();

        Self {
            rel_path_without_gpg,
            plain_root,
            gpg_root,
        }
    }

    pub fn as_plain(&self) -> PathBuf {
        self.plain_root.join(&self.rel_path_without_gpg)
    }

    pub fn as_gpg(&self) -> PathBuf {
        add_gpg_extension(&self.gpg_root.join(&self.rel_path_without_gpg))
    }

    pub fn rel_without_gpg(&self) -> &PathBuf {
        &self.rel_path_without_gpg
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_representation() {
        // TODO the from_* and as_* methods shall work correctly
    }
}
