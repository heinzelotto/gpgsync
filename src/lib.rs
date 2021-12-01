use std::collections::HashSet;
use std::ffi::OsStr;
//use std::fs::{DirEntry, File};
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::anyhow;

use filesync::{FileStatus, SyncAction};
use syncdb::SyncDb;
use syncentity::SyncEntity;

mod fileread;
mod filesync;
mod fileutils;
mod gpg;
pub mod rewrite;
mod syncdb;
mod syncentity;

// /// File name of the database.  Will be saved inside the plain root directory.
// const DB_FILENAME: &str = ".gpgsyncdb";

// /// Delay for which filesystem events are held back to e. g. clean up duplicates.
// const WATCHER_DEBOUNCE_DURATION: Duration = Duration::from_secs(1);

// /// The GPGsync instance.
// pub struct GpgSync {
//     /// The sync database is persisted in the `plain_root` across program runs.
//     db: SyncDb,
//     /// Full path where the DB is stored.
//     db_path: PathBuf,
//     /// Directory containing all unencrypted files.
//     plain_root: PathBuf,
//     /// Directory containing all encrypted files.
//     gpg_root: PathBuf,
//     /// Passphrase used for all encryption.
//     passphrase: String,
//     /// Channel to receive all file watcher events on.
//     rx: std::sync::mpsc::Receiver<notify::DebouncedEvent>,
//     /// The file watcher.  Must be kept alive while the program is running
//     _watcher: notify::RecommendedWatcher,
// }

// impl GpgSync {
//     /// Returns a new GPGsync.
//     ///
//     /// When constructing a new GPGsync, an existing database is loaded if
//     /// existing. An initial sync is performed and the file watcher is started,
//     /// whose events can be processed by calls to `try_process_events()`.
//     pub fn new(plain_root: &Path, gpg_root: &Path, passphrase: &str) -> anyhow::Result<Self> {
//         use notify::Watcher;

//         let plain_root = std::fs::canonicalize(plain_root)?;
//         let gpg_root = std::fs::canonicalize(gpg_root)?;

//         validate_args(&plain_root, &gpg_root)?;

//         let db_path = &plain_root.join(DB_FILENAME);

//         let mut db = SyncDb::load_db(db_path).unwrap_or(SyncDb::new(&gpg_root));
//         if db.gpg_root() != gpg_root {
//             // TODO just delete the db in this case
//             return Err(anyhow!(
//                 "existing database for another gpg_root found, unsupported"
//             ));
//         }

//         // TODO read .gitignore

//         let mut ses = HashSet::new();
//         fileutils::visit_dir(&plain_root, &mut |de| {
//             if !is_hidden(&de.path()) {
//                 let se = SyncEntity::from_plain(&de.path(), &plain_root, &gpg_root);
//                 ses.insert(se);
//             } else {
//                 println!("filtered file {:?}", &de.path());
//             }
//         })?;

//         fileutils::visit_dir(&gpg_root, &mut |de| {
//             if !is_hidden(&de.path()) {
//                 // TODO enhance ignoring of files
//                 if de.path().extension() == Some(OsStr::new("gpg")) {
//                     let se = SyncEntity::from_gpg(&de.path(), &plain_root, &gpg_root);
//                     ses.insert(se);
//                 } else {
//                     println!("In gpg dir, skipping non-.gpg file: {:?}", de)
//                 }
//             } else {
//                 println!("filtered file {:?}", &de.path());
//             }
//         })?;

//         // TODO: also add files from db to ses

//         for se in ses {
//             let sync_action = analyze_file_and_update_db(&mut db, &se)?;
//             println!("SyncAction {:?} {:?}", &se, sync_action);

//             perform_sync_action_and_update_db(sync_action, &se, &mut db, &passphrase)?;
//             db.save_db(&db_path);
//         }

//         // TODO init watcher even before initial sync!

//         let (tx, rx) = std::sync::mpsc::channel();
//         let mut watcher = notify::watcher(tx, WATCHER_DEBOUNCE_DURATION)?;

//         watcher.watch(&plain_root, notify::RecursiveMode::Recursive)?;
//         watcher.watch(&gpg_root, notify::RecursiveMode::Recursive)?;

//         Ok(Self {
//             db,
//             db_path: db_path.clone(),
//             plain_root: plain_root.clone(),
//             gpg_root: gpg_root.clone(),
//             passphrase: passphrase.to_string(),
//             rx,
//             _watcher: watcher,
//         })
//     }

//     /// Receive new events from the file watcher and perform sync actions if necessary.
//     ///
//     /// The functions blocks for at most `timeout` until an event is received or
//     /// the watcher terminates.
//     pub fn try_process_events(&mut self, timeout: Duration) -> anyhow::Result<()> {
//         match self.rx.recv_timeout(timeout) {
//             Ok(event) => {
//                 println!("event {:?}", event);
//                 match event {
//                     notify::DebouncedEvent::NoticeWrite(_)
//                     | notify::DebouncedEvent::NoticeRemove(_) => {
//                         println!("noticed begin of write or remove");
//                     }
//                     notify::DebouncedEvent::Create(p)
//                     | notify::DebouncedEvent::Write(p)
//                     | notify::DebouncedEvent::Remove(p) => {
//                         self.do_sync_path(dbg!(&p))?;
//                     }
//                     notify::DebouncedEvent::Chmod(_) => {
//                         println!("chmod");
//                     }
//                     notify::DebouncedEvent::Rename(p_src, p_dst) => {
//                         println!("Rename event, from {:?} to {:?}", p_src, p_dst);
//                         // we don't support moving between the two directories
//                         // TODO ?why not
//                         if !((p_src.starts_with(&self.plain_root)
//                             && p_dst.starts_with(&self.plain_root))
//                             || (p_src.starts_with(&self.gpg_root)
//                                 && p_dst.starts_with(&self.gpg_root)))
//                         {
//                             return Err(anyhow!(
//                                 "moving between the two directories not supported"
//                             ));
//                         }

//                         // don't do anything smart for now. Just trigger two sync actions, on p_src and p_dst
//                         self.do_sync_path(&p_src)?;
//                         self.do_sync_path(&p_dst)?;
//                     }
//                     notify::DebouncedEvent::Rescan => {}
//                     notify::DebouncedEvent::Error(e, po) => {
//                         println!("error on path {:?}: {}", po, e);
//                     }
//                 }
//             }
//             Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
//             Err(_) => return Err(anyhow!("watcher died.")),
//         }

//         Ok(())
//     }

//     /// Analyze a file at a path and perform a sync action if necessary.
//     fn do_sync_path(&mut self, p: &Path) -> anyhow::Result<()> {
//         if !is_hidden(&p) {
//             // TODO enhance ignoring of files
//             let se = if p.starts_with(dbg!(&self.plain_root)) {
//                 SyncEntity::from_plain(&p.to_path_buf(), &self.plain_root, &self.gpg_root)
//             } else {
//                 SyncEntity::from_gpg(&p.to_path_buf(), &self.plain_root, &self.gpg_root)
//             };
//             let sync_action = analyze_file_and_update_db(&mut self.db, &se)?;
//             println!("{:?} {:?}", &p, sync_action);

//             perform_sync_action_and_update_db(
//                 sync_action,
//                 &se,
//                 &mut self.db,
//                 &self.passphrase, // could be chosen per file as well
//             )?;
//             self.db.save_db(&self.db_path);
//         } else {
//             println!("filtered file {:?}", &p);
//         }

//         Ok(())
//     }
// }

// fn validate_args(plain_root: &PathBuf, gpg_root: &PathBuf) -> anyhow::Result<()> {
//     if plain_root.starts_with(&gpg_root) || gpg_root.starts_with(&plain_root) {
//         return Err(anyhow!("The two paths must not contain each other."));
//     }

//     if !plain_root.exists() {
//         return Err(anyhow!(format!("No such directory: {:?}", plain_root)));
//     }
//     if !gpg_root.exists() {
//         return Err(anyhow!(format!("No such directory: {:?}", gpg_root)));
//     }

//     Ok(())
// }

// fn file_statuses(se: &SyncEntity) -> io::Result<(FileStatus, FileStatus)> {
//     Ok((
//         fileutils::file_status(&se.as_plain())?,
//         fileutils::file_status(&se.as_gpg())?,
//     ))
// }

// pub fn check_coincide(se: &SyncEntity, passphrase: &str) -> bool {
//     let gpg_hash = gpg_file_hash(&se.as_gpg(), passphrase).unwrap();
//     let plain_hash = plain_file_hash(&se.as_plain()).unwrap();
//     gpg_hash == plain_hash
// }

// pub fn push_plain(se: &SyncEntity, passphrase: &str) -> io::Result<()> {
//     let mut plain_f = fileutils::open_read(&se.as_plain())?;

//     let mut gpg_f = fileutils::open_write(&se.as_gpg())?;

//     crate::gpg::encrypt(&mut plain_f, &mut gpg_f, passphrase.as_bytes())?;

//     Ok(())
// }

// pub fn push_gpg(se: &SyncEntity, passphrase: &str) -> io::Result<()> {
//     let mut gpg_f = fileutils::open_read(&se.as_gpg())?;

//     let mut plain_f = fileutils::open_write(&se.as_plain())?;

//     crate::gpg::decrypt(&mut gpg_f, &mut plain_f, passphrase.as_bytes()).unwrap();

//     Ok(())
// }
// pub fn hash_all(p: &mut impl Read) -> io::Result<Vec<u8>> {
//     use md5::Digest;

//     let mut hasher = md5::Md5::new();

//     let mut buf: [u8; 1024] = [0; 1024];
//     loop {
//         let n = p.read(&mut buf)?;
//         if n == 0 {
//             break;
//         }

//         hasher.update(&buf[0..n]);
//     }

//     Ok(hasher.finalize().to_vec())
// }

// pub fn plain_file_hash(p: &Path) -> io::Result<Vec<u8>> {
//     let mut f = fileutils::open_read(p)?;

//     hash_all(&mut f)
// }

// fn gpg_file_hash(p: &Path, passphrase: &str) -> io::Result<Vec<u8>> {
//     let mut f = fileutils::open_read(p)?;

//     let mut decrypted = Vec::new();

//     gpg::decrypt(&mut f, &mut decrypted, passphrase.as_bytes())?;

//     hash_all(&mut Cursor::new(decrypted))
// }

// fn analyze_file_and_update_db(db: &mut SyncDb, se: &SyncEntity) -> io::Result<SyncAction> {
//     let (plain_status_prev, gpg_status_prev) = db.get_file_status(&se);

//     let plain_status_cur = dbg!(fileutils::file_status(&se.as_plain()))?;
//     let gpg_status_cur = dbg!(fileutils::file_status(&se.as_gpg()))?;

//     let sync_action = filesync::determine_sync_action(
//         filesync::determine_file_change(plain_status_prev, plain_status_cur),
//         filesync::determine_file_change(gpg_status_prev, gpg_status_cur),
//     );

//     db.set_file_status(&se, plain_status_cur, gpg_status_cur);

//     Ok(sync_action)
// }

// fn perform_sync_action_and_update_db(
//     sync_action: SyncAction,
//     se: &SyncEntity,
//     db: &mut SyncDb,
//     passphrase: &str,
// ) -> io::Result<()> {
//     match sync_action {
//         SyncAction::None => {}
//         SyncAction::PossibleConflict => {
//             if !check_coincide(se, passphrase) {
//                 println!("conflict {:?}", &se);
//             // todo mark it as conflicted in db
//             } else {
//                 println!("No Conflict!");
//             }
//         }
//         SyncAction::PushPlain => {
//             push_plain(se, passphrase)?;
//         }
//         SyncAction::DeletePlain => {
//             std::fs::remove_file(se.as_plain())?;
//         }
//         SyncAction::PushGpg => {
//             push_gpg(se, passphrase)?;
//         }
//         SyncAction::DeleteGpg => {
//             std::fs::remove_file(&se.as_gpg())?;
//         }
//     }
//     let (plain_status, gpg_status) = file_statuses(se)?;
//     db.set_file_status(&se, plain_status, gpg_status);

//     Ok(())
// }

// fn is_hidden(p: &std::path::Path) -> bool {
//     for os_s in p {
//         let s = os_s.to_string_lossy();
//         if s.starts_with(".") && s != "." && s != ".." {
//             return true;
//         }
//     }
//     false
// }

// // #[cfg(test)]
// // mod test {

// //     use super::GpgSync;

// //     use lazy_static::lazy_static;
// //     use std::io::Write;
// //     use std::path::{Path, PathBuf};
// //     use std::time::Duration;

// //     fn poll_predicate(p: &mut dyn FnMut() -> bool, timeout: Duration) {
// //         let mut remaining = Some(timeout);
// //         let decrement = Duration::new(0, 3_000_000);
// //         loop {
// //             if let Some(rem) = remaining {
// //                 remaining = rem.checked_sub(decrement);
// //             } else {
// //                 break;
// //             }

// //             if p() {
// //                 return;
// //             }

// //             std::thread::sleep(decrement);
// //         }
// //         panic!("predicate did not evaluate to true within {:?}", timeout);
// //     }

// //     lazy_static! {
// //         static ref PLAIN_ROOT: &'static Path = &Path::new("./plain_root");
// //         static ref GPG_ROOT: &'static Path = &Path::new("./gpg_root");
// //     }

// //     fn test_roots(test_name: &str) -> (PathBuf, PathBuf) {
// //         (PLAIN_ROOT.join(test_name), GPG_ROOT.join(test_name))
// //     }

// //     fn init_dir(p: &Path) {
// //         if p.exists() {
// //             std::fs::remove_dir_all(&p)?;
// //         }
// //         std::fs::create_dir_all(&p)?;
// //     }

// //     fn init_dirs(pr: &Path, gr: &Path) {
// //         init_dir(pr);
// //         init_dir(gr);
// //     }

// //     fn make_file(p: &Path, s: &[u8]) {
// //         let mut f = std::fs::OpenOptions::new()
// //             .create_new(true)
// //             .write(true)
// //             .open(p)?;
// //         f.write_all(s)?;
// //     }

// //     #[test]
// //     fn test_creation_failure() {
// //         // TODO one dir is inside the other

// //         // TODO dir doesn't exist
// //     }

// //     #[test]
// //     fn test_basic() {
// //         let (pr, gr) = test_roots("test_basic");

// //         // PD/notes.txt -> GD/notes.txt.gpg
// //         {
// //             init_dirs(&pr, &gr);
// //             make_file(&pr.join("notes.txt"), b"hello");
// //             let _gpgs = GpgSync::new(&pr, &gr, "test")?;
// //             assert!(gr.join("notes.txt.gpg").exists());
// //         }

// //         // GD/notes.txt.gpg -> PD/notes.txt
// //         {
// //             init_dirs(&pr, &gr);
// //             make_file(&gr.join("notes.txt.gpg"), include_bytes!("notes.txt.gpg"));
// //             let _gpgs = GpgSync::new(&pr, &gr, "test")?;
// //             assert!(pr.join("notes.txt").exists());
// //         }
// //     }

// //     #[test]
// //     #[should_panic]
// //     fn test_wrong_passphrase() {
// //         let (pr, gr) = test_roots("test_wrong_passphrase");
// //         init_dirs(&pr, &gr);
// //         make_file(&gr.join("notes.txt.gpg"), include_bytes!("notes.txt.gpg"));
// //         let _gpgs = GpgSync::new(&pr, &gr, "test_wrong_passphrase")?;
// //     }

// //     #[test]
// //     fn test_directory_deletion() {
// //         // TODO PLAIN_ROOT gets deleted
// //         // TODO GPG_ROOT gets deleted
// //     }

// //     #[test]
// //     fn test_conflict() {
// //         // all of this logic is supposed to be tested in filesync

// //         // TODO panic when both are changed/added/modified and incompatible
// //         // TODO do nothing when both are changed/added/modified but the same
// //     }

// //     #[test]
// //     fn test_graceful_conflict() {
// //         // TODO Add plain + Del gpg -> pushplain
// //     }

// //     #[test]
// //     fn test_rename() {
// //         // TODO check failure when the target file exists
// //         // TODO check that moving from one directory into the other is not allowed
// //         let (pr, gr) = test_roots("test_rename");

// //         init_dirs(&pr, &gr);
// //         make_file(&pr.join("notes.txt"), b"hello");
// //         let mut gpgs = GpgSync::new(&pr, &gr, "test")?;
// //         assert!(gr.join("notes.txt.gpg").exists());

// //         std::fs::rename(pr.join("notes.txt"), pr.join("notes_renamed.txt"))?;

// //         poll_predicate(
// //             &mut || {
// //                 gpgs.try_process_events(Duration::new(0, 200_000_000))?;

// //                 !gr.join("notes.txt.gpg").exists() && gr.join("notes_renamed.txt.gpg").exists()
// //             },
// //             Duration::new(2, 0),
// //         );
// //     }

// //     #[test]
// //     fn test_running_sync() {
// //         let (pr, gr) = test_roots("test_running_sync");

// //         init_dirs(&pr, &gr);
// //         let mut gpgs = GpgSync::new(&pr, &gr, "test")?;

// //         assert!(!gr.join("notes.txt.gpg").exists());

// //         make_file(&pr.join("notes.txt"), b"hello");
// //         poll_predicate(
// //             &mut || {
// //                 gpgs.try_process_events(Duration::new(0, 200_000_000))?;

// //                 gr.join("notes.txt.gpg").exists()
// //             },
// //             Duration::new(2, 0),
// //         );
// //     }

// //     #[test]
// //     fn test_database() {
// //         // TODO do some syncs, quit, modify plain, start again, and no conflict but pushplain should happen!
// //     }

// //     #[test]
// //     #[should_panic]
// //     fn test_changed_gpgroot() {
// //         let (pr, gr) = test_roots("test_changed_gpgroot");
// //         init_dirs(&pr, &gr);
// //         make_file(&pr.join("notes.txt"), b"hello");
// //         let gpgs = GpgSync::new(&pr, &gr, "test")?;
// //         assert!(gr.join("notes.txt.gpg").exists());
// //         std::mem::drop(gpgs);

// //         let (_, gr2) = test_roots("test_changed_gpgroot2");
// //         init_dir(&gr2);
// //         let _gpgs = GpgSync::new(&pr, &gr2, "test")?;
// //     }
// // }
