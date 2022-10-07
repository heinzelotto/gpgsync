use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lazy_static::lazy_static;
#[cfg(test)]
pub fn poll_predicate(p: &mut dyn FnMut() -> bool, timeout: Duration) {
    let start = Instant::now();
    let decrement = Duration::from_millis(5);
    loop {
        if Instant::now() >= start + timeout {
            break;
        }

        if p() {
            return;
        }

        std::thread::sleep(decrement);
    }
    panic!("predicate did not evaluate to true within {:?}", timeout);
}

#[cfg(test)]
lazy_static! {
    static ref PLAIN_ROOT: &'static Path = &Path::new("./plain_root");
    static ref GPG_ROOT: &'static Path = &Path::new("./gpg_root");
}

#[cfg(test)]
pub fn test_roots(test_name: &str) -> (PathBuf, PathBuf) {
    (PLAIN_ROOT.join(test_name), GPG_ROOT.join(test_name))
}

#[cfg(test)]
pub fn init_dir(p: &Path) -> anyhow::Result<()> {
    if p.exists() {
        std::fs::remove_dir_all(&p)?;
    }
    std::fs::create_dir_all(&p)?;

    Ok(())
}

#[cfg(test)]
pub fn init_dirs(pr: &Path, gr: &Path) {
    init_dir(pr);
    init_dir(gr);
}

#[cfg(test)]
pub fn make_file(p: &Path, s: &[u8]) -> anyhow::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(p)?;
    f.write_all(s)?;

    assert!(p.exists());

    Ok(())
}

#[cfg(test)]
pub fn make_encrypted_file(p: &Path, s: &[u8], passphrase: &str) -> anyhow::Result<()> {
    let mut tmpfile = tempfile::NamedTempFile::new()?;
    tmpfile.as_file().write_all(s)?;
    crate::rewrite::fs_ops::encrypt_file(tmpfile.path(), p, passphrase)?;

    Ok(())
}
#[cfg(test)]
macro_rules! function_name {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);
        &name[..name.len() - 3]
    }};
}
#[cfg(test)]
pub(crate) use function_name;
