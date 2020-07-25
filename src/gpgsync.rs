use anyhow::{anyhow, Result};
use notify::{watcher, DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::fs::{self, DirEntry};
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::{collections::HashSet, ffi::OsStr}; // todo crossbeam
                                             // use std::str::from_utf8;
use std::time::Duration;

use md5::{Digest, Md5};
use std::sync::mpsc;

use crate::gpg::*;

const DB_FILENAME: &str = ".gpgsyncdb";

use crate::syncdb::*;

use crate::filesync::*;

use crate::syncentity::*;

fn validate_args(plain_root: &PathBuf, gpg_root: &PathBuf) -> Result<()> {
    if plain_root.starts_with(&gpg_root) || gpg_root.starts_with(&plain_root) {
        return Err(anyhow!("The two paths must not contain each other."));
    }

    if !plain_root.exists() {
        return Err(anyhow!(format!("No such directory: {:?}", plain_root)));
    }
    if !gpg_root.exists() {
        return Err(anyhow!(format!("No such directory: {:?}", gpg_root)));
    }

    Ok(())
}

fn visit_dir(dir: &Path, cb: &mut dyn FnMut(&DirEntry)) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
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
pub fn file_status(fp: &PathBuf) -> FileStatus {
    if !fp.exists() {
        return FileStatus::Nonexistent;
    }

    let mtime = std::fs::metadata(&fp).unwrap().modified().unwrap();
    FileStatus::Existent(mtime)
}

pub fn file_statuses(se: &SyncEntity) -> (FileStatus, FileStatus) {
    (file_status(&se.as_plain()), file_status(&se.as_gpg()))
}

pub fn check_coincide(se: &SyncEntity, passphrase: &str) -> bool {
    let gpg_hash = gpg_file_hash(&se.as_gpg(), passphrase).unwrap();
    let plain_hash = plain_file_hash(&se.as_plain()).unwrap();
    gpg_hash == plain_hash
}

pub fn push_plain(se: &SyncEntity, passphrase: &str) {
    //    dbg!(&plain_root.join(fp));
    let mut plain_f = File::open(&se.as_plain()).unwrap();

    //  dbg!(&add_gpg_extension(&gpg_root.join(&fp)));

    //dbg!(&gpg_data);
    let mut gpg_f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&se.as_gpg())
        .unwrap();

    crate::gpg::encrypt(&mut plain_f, &mut gpg_f, passphrase.as_bytes()).unwrap();
}

pub fn push_gpg(se: &SyncEntity, passphrase: &str) {
    let mut gpg_f = File::open(&se.as_gpg()).unwrap();

    let mut plain_f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&se.as_plain())
        .unwrap();

    crate::gpg::decrypt(&mut gpg_f, &mut plain_f, passphrase.as_bytes()).unwrap();
}
pub fn hash_all(p: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut hasher = Md5::new();

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

pub fn plain_file_hash(p: &Path) -> io::Result<Vec<u8>> {
    let mut f = File::open(p)?;

    hash_all(&mut f)
}

fn gpg_file_hash(p: &Path, passphrase: &str) -> io::Result<Vec<u8>> {
    let mut f = File::open(p)?;

    let mut decrypted = Vec::new();

    decrypt(&mut f, &mut decrypted, passphrase.as_bytes()).unwrap();

    //println!("{:?}", from_utf8(&df));

    hash_all(&mut Cursor::new(decrypted))
}
// #[derive(Debug)]
// enum SyncHash {
//     Agree(Vec<u8>),
//     Conflict,
// }

// struct Revision(u64);

fn analyze_file_and_update_db(db: &mut SyncDb, se: &SyncEntity) -> SyncAction {
    let (plain_status_prev, gpg_status_prev) = db.get_file_status(&se);

    //dbg!(plain_status_prev);
    //dbg!(gpg_status_prev);

    let plain_status_cur = dbg!(file_status(&se.as_plain()));
    let gpg_status_cur = dbg!(file_status(&se.as_gpg()));

    let sync_action = determine_sync_action(
        determine_file_change(plain_status_prev, plain_status_cur),
        determine_file_change(gpg_status_prev, gpg_status_cur),
    );

    db.set_file_status(&se, plain_status_cur, gpg_status_cur);

    // persist store db to disk

    sync_action
}

fn perform_sync_action_and_update_db(
    sync_action: SyncAction,
    se: &SyncEntity,
    db: &mut SyncDb,
    passphrase: &str,
) {
    match sync_action {
        SyncAction::None => {}
        SyncAction::PossibleConflict => {
            if !check_coincide(se, passphrase) {
                println!("conflict {:?}", &se);
            // todo mark it as conflicted in db
            } else {
                println!("No Conflict!");
            }
        }
        SyncAction::PushPlain => {
            push_plain(se, passphrase);
        }
        SyncAction::DeletePlain => {
            std::fs::remove_file(se.as_plain()).unwrap();
        }
        SyncAction::PushGpg => {
            push_gpg(se, passphrase);
        }
        SyncAction::DeleteGpg => {
            std::fs::remove_file(&se.as_gpg()).unwrap();
        }
    }
    let (plain_status, gpg_status) = file_statuses(se);
    db.set_file_status(&se, plain_status, gpg_status);
}

//fn handle_file_change(fp: &PathBuf) {}

fn is_hidden(p: &std::path::Path) -> bool {
    for os_s in p {
        let s = os_s.to_string_lossy();
        if s.starts_with(".") && s != "." && s != ".." {
            return true;
        }
    }
    false
}
pub struct GpgSync {
    db: SyncDb,
    db_path: PathBuf,
    plain_root: PathBuf,
    gpg_root: PathBuf,
    passphrase: String,
    rx: mpsc::Receiver<DebouncedEvent>,
    _watcher: RecommendedWatcher,
}

impl GpgSync {
    pub fn new(plain_root: &Path, gpg_root: &Path, passphrase: &str) -> Result<Self> {
        let plain_root = std::fs::canonicalize(plain_root).unwrap();
        let gpg_root = std::fs::canonicalize(gpg_root).unwrap();

        validate_args(&plain_root, &gpg_root).unwrap();

        let db_path = &plain_root.join(&Path::new(DB_FILENAME));

        let mut db = SyncDb::load_db(db_path).unwrap_or(SyncDb::new(&gpg_root));
        assert!(db.gpg_root() == gpg_root);

        // read .gitignore

        let mut ses = HashSet::new();
        visit_dir(&plain_root, &mut |de| {
            if !is_hidden(&de.path()) {
                let se = SyncEntity::from_plain(&de.path(), &plain_root, &gpg_root);
                ses.insert(se);
            } else {
                println!("filtered file {:?}", &de.path());
            }
        })
        .unwrap();

        visit_dir(&gpg_root, &mut |de| {
            if !is_hidden(&de.path()) {
                // todo make more universal
                if de.path().extension() == Some(OsStr::new("gpg")) {
                    let se = SyncEntity::from_gpg(&de.path(), &plain_root, &gpg_root);
                    ses.insert(se);
                } else {
                    println!("In gpg dir, skipping non-.gpg file: {:?}", de)
                }
            } else {
                println!("filtered file {:?}", &de.path());
            }
        })
        .unwrap();

        // TODO: also add files from db to ses

        for se in ses {
            let sync_action = analyze_file_and_update_db(&mut db, &se);
            println!("{:?} {:?}", &se, sync_action);
            //db.save_db(&db_path);

            perform_sync_action_and_update_db(sync_action, &se, &mut db, &passphrase);
            db.save_db(&db_path);
        }

        // todo init watcher even before initial sync!

        let (tx, rx) = channel();
        // TODO move to other thread
        let mut watcher = watcher(tx, Duration::from_secs(1)).unwrap();

        watcher
            .watch(&plain_root, RecursiveMode::Recursive)
            .unwrap();
        watcher.watch(&gpg_root, RecursiveMode::Recursive).unwrap();

        Ok(Self {
            db,
            db_path: db_path.clone(),
            plain_root: plain_root.clone(),
            gpg_root: gpg_root.clone(),
            passphrase: passphrase.to_string(),
            rx,
            _watcher: watcher,
        })
    }

    fn sync_path(&mut self, p: &Path) {
        if !is_hidden(&p) {
            // todo make more universal
            let se = if p.starts_with(dbg!(&self.plain_root)) {
                // todo if is not ignored plain file
                SyncEntity::from_plain(&p.to_path_buf(), &self.plain_root, &self.gpg_root)
            } else {
                // todo if is not ignored gpg file
                SyncEntity::from_gpg(&p.to_path_buf(), &self.plain_root, &self.gpg_root)
            };
            let sync_action = analyze_file_and_update_db(&mut self.db, &se);
            //self.db.save_db(&self.db_path);
            println!("{:?} {:?}", &p, sync_action);

            perform_sync_action_and_update_db(
                sync_action,
                &se,
                &mut self.db,
                &self.passphrase, // could be chosen per file as well
            );
            self.db.save_db(&self.db_path);
        } else {
            println!("filtered file {:?}", &p);
        }
    }

    pub fn try_process_events(&mut self) {
        match self.rx.try_recv() {
            Ok(event) => {
                //println!("db: {:?}", &db);
                println!("event {:?}", event);
                match event {
                    DebouncedEvent::NoticeWrite(_) | DebouncedEvent::NoticeRemove(_) => {
                        println!("noticed begin of write or remove");
                    }
                    DebouncedEvent::Create(p)
                    | DebouncedEvent::Write(p)
                    | DebouncedEvent::Remove(p) => {
                        self.sync_path(dbg!(&p));
                    }
                    DebouncedEvent::Chmod(_) => {
                        println!("chmod");
                    }
                    DebouncedEvent::Rename(p_src, p_dst) => {
                        println!("Rename event, from {:?} to {:?}", p_src, p_dst);
                        // we don't support moving between the two directories
                        assert!(
                            !(p_src.starts_with(&self.plain_root)
                                ^ p_dst.starts_with(&self.plain_root))
                        );

                        // don't do anything smart for now. Just trigger two sync actions, on p_src and p_dst
                        self.sync_path(&p_src);
                        self.sync_path(&p_dst);
                    }
                    DebouncedEvent::Rescan => {}
                    DebouncedEvent::Error(e, po) => {
                        println!("error on path {:?}: {}", po, e);
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(e) => println!("file watch error: {:?}", e),
        }
    }
}

#[cfg(test)]
mod test {

    use lazy_static::lazy_static;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    fn poll_predicate(p: &mut dyn FnMut() -> bool, timeout: Duration) {
        let mut remaining = Some(timeout);
        let decrement = Duration::new(0, 3_000_000);
        loop {
            if let Some(rem) = remaining {
                remaining = rem.checked_sub(decrement);
            } else {
                break;
            }

            if p() {
                return;
            }

            std::thread::sleep(decrement);
        }
        panic!("predicate did not evaluate to true within {:?}", timeout);
    }

    lazy_static! {
        static ref PLAIN_ROOT: &'static Path = &Path::new("./plain_root");
        static ref GPG_ROOT: &'static Path = &Path::new("./gpg_root");
    }

    fn test_roots(test_name: &str) -> (PathBuf, PathBuf) {
        (PLAIN_ROOT.join(test_name), GPG_ROOT.join(test_name))
    }

    fn init_dir(p: &Path) {
        if p.exists() {
            fs::remove_dir_all(&p).unwrap();
        }
        fs::create_dir_all(&p).unwrap();
    }

    fn init_dirs(pr: &Path, gr: &Path) {
        init_dir(pr);
        init_dir(gr);
    }

    fn make_file(p: &Path, s: &[u8]) {
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(p)
            .unwrap();
        f.write_all(s).unwrap();
    }

    #[test]
    fn test_creation_failure() {
        // TODO one dir is inside the other

        // TODO dir doesn't exist
    }

    #[test]
    fn test_basic() {
        let (pr, gr) = test_roots("test_basic");

        // PD/notes.txt -> GD/notes.txt.gpg
        {
            init_dirs(&pr, &gr);
            make_file(&pr.join("notes.txt"), b"hello");
            let _gpgs = super::GpgSync::new(&pr, &gr, "test").unwrap();
            assert!(gr.join("notes.txt.gpg").exists());
        }

        // GD/notes.txt.gpg -> PD/notes.txt
        {
            init_dirs(&pr, &gr);
            make_file(&gr.join("notes.txt.gpg"), include_bytes!("notes.txt.gpg"));
            let _gpgs = super::GpgSync::new(&pr, &gr, "test").unwrap();
            assert!(pr.join("notes.txt").exists());
        }
    }

    #[test]
    #[should_panic]
    fn test_wrong_passphrase() {
        let (pr, gr) = test_roots("test_wrong_passphrase");
        init_dirs(&pr, &gr);
        make_file(&gr.join("notes.txt.gpg"), include_bytes!("notes.txt.gpg"));
        let _gpgs = super::GpgSync::new(&pr, &gr, "test_wrong_passphrase").unwrap();
    }

    #[test]
    fn test_directory_deletion() {
        // TODO PLAIN_ROOT gets deleted
        // TODO GPG_ROOT gets deleted
    }

    #[test]
    fn test_conflict() {
        // all of this logic is supposed to be tested in filesync

        // TODO panic when both are changed/added/modified and incompatible
        // TODO do nothing when both are changed/added/modified but the same
    }

    #[test]
    fn test_graceful_conflict() {
        // TODO Add plain + Del gpg -> pushplain
    }

    #[test]
    fn test_rename() {
        // TODO check failure when the target file exists
        // TODO check that moving from one directory into the other is not allowed
        let (pr, gr) = test_roots("test_rename");

        init_dirs(&pr, &gr);
        make_file(&pr.join("notes.txt"), b"hello");
        let mut gpgs = super::GpgSync::new(&pr, &gr, "test").unwrap();
        assert!(gr.join("notes.txt.gpg").exists());

        std::fs::rename(pr.join("notes.txt"), pr.join("notes_renamed.txt")).unwrap();

        poll_predicate(
            &mut || {
                gpgs.try_process_events();

                !gr.join("notes.txt.gpg").exists() && gr.join("notes_renamed.txt.gpg").exists()
            },
            Duration::new(2, 0),
        );
    }

    #[test]
    fn test_running_sync() {
        let (pr, gr) = test_roots("test_running_sync");

        init_dirs(&pr, &gr);
        let mut gpgs = super::GpgSync::new(&pr, &gr, "test").unwrap();

        assert!(!gr.join("notes.txt.gpg").exists());

        make_file(&pr.join("notes.txt"), b"hello");
        poll_predicate(
            &mut || {
                gpgs.try_process_events();

                gr.join("notes.txt.gpg").exists()
            },
            Duration::new(2, 0),
        );
    }

    #[test]
    fn test_database() {
        // TODO do some syncs, quit, modify plain, start again, and no conflict but pushplain should happen!
    }

    #[test]
    #[should_panic]
    fn test_changed_gpgroot() {
        let (pr, gr) = test_roots("test_changed_gpgroot");
        init_dirs(&pr, &gr);
        make_file(&pr.join("notes.txt"), b"hello");
        let gpgs = super::GpgSync::new(&pr, &gr, "test").unwrap();
        assert!(gr.join("notes.txt.gpg").exists());
        std::mem::drop(gpgs);

        let (_, gr2) = test_roots("test_changed_gpgroot2");
        init_dir(&gr2);
        let _gpgs = super::GpgSync::new(&pr, &gr2, "test").unwrap();
    }
}
