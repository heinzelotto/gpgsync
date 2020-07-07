use anyhow::{anyhow, Result};
use md5::{Digest, Md5};
use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{self, DirEntry, File};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel; // todo crossbeam
                              // use std::str::from_utf8;
use std::time::Duration;
use structopt::StructOpt;

mod gpg;

type SyncDb = HashMap<std::path::PathBuf, (FileStatus, FileStatus)>;

#[derive(StructOpt)]
struct Cli {
    /// The plaintext data path
    #[structopt(parse(from_os_str))]
    plain_root: std::path::PathBuf, // plain_base_path
    /// The encrypted gpg path
    #[structopt(parse(from_os_str))]
    gpg_root: std::path::PathBuf, // gpg_base_path
    /// The passphrase
    passphrase: String,
}

fn validate_args(args: &Cli) -> Result<()> {
    if args.plain_root.starts_with(&args.gpg_root) || args.gpg_root.starts_with(&args.plain_root) {
        return Err(anyhow!("The two paths must not contain each other."));
    }

    if !args.plain_root.exists() {
        return Err(anyhow!(format!("No such directory: {:?}", args.plain_root)));
    }
    if !args.gpg_root.exists() {
        return Err(anyhow!(format!("No such directory: {:?}", args.gpg_root)));
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

fn hash_all(p: &mut impl Read) -> io::Result<Vec<u8>> {
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

fn plain_file_hash(p: &Path) -> io::Result<Vec<u8>> {
    let mut f = File::open(p)?;

    hash_all(&mut f)
}

fn gpg_file_hash(p: &Path, passphrase: &str) -> io::Result<Vec<u8>> {
    let mut f = File::open(p)?;
    // let mut buf: [u8; 1024] = [0; 1024];
    // loop {
    //     let n = f.read(&mut buf)?;
    //     if n == 0 {
    //         break;
    //     }

    //     hasher.update(&buf[0..n]);
    // }

    let df = gpg::decrypt(&mut f, passphrase.as_bytes()).unwrap();

    //println!("{:?}", from_utf8(&df));

    hash_all(&mut Cursor::new(df))
}

/// just a partition of relative paths, without checking for conflicts
#[derive(Debug)]
struct GpgDirPathDiff<'a> {
    plain_only: Vec<&'a std::path::PathBuf>,
    gpg_only: Vec<&'a std::path::PathBuf>,
    both: Vec<&'a std::path::PathBuf>,
}

fn compare_sets<'a>(
    plain_paths: &'a HashSet<std::path::PathBuf>,
    gpg_paths: &'a HashSet<std::path::PathBuf>,
) -> GpgDirPathDiff<'a> {
    let plain_only: Vec<&'a std::path::PathBuf> = plain_paths.difference(&gpg_paths).collect();
    let gpg_only: Vec<&'a std::path::PathBuf> = gpg_paths.difference(&plain_paths).collect();
    let both: Vec<&'a std::path::PathBuf> = plain_paths.intersection(&gpg_paths).collect();

    GpgDirPathDiff {
        plain_only,
        gpg_only,
        both,
    }

    // println!("s1_only\n{:?}", &s1_only);
    // println!("s2_only\n{:?}", &s2_only);
    // println!("both\n{:?}", &both);

    // let conflicts: Vec<&std::path::PathBuf> = both.iter().copied().map(|p| p).collect();
    // if !conflicts.is_empty() {
    //     for p in conflicts {
    //         println!("{:#?} is out of sync", p);
    //     }
    //     return Err(anyhow!("Some files are out of sync"));
    // }

    // quit if some are out of sync

    // s1_only encrypt to s2 (but outside this function, do only the analysis here)

    // s2_only decrypt to s1. (at this point it will already have been decrypted,
    // but we have to do it twice if we don't add temporary files)

    //Ok(())
}

#[derive(Debug)]
enum SyncHash {
    Agree(Vec<u8>),
    Conflict,
}

fn add_gpg_extension(p: &std::path::PathBuf) -> std::path::PathBuf {
    let mut name = p.file_name().unwrap().to_owned();
    name.push(".gpg");
    p.parent().unwrap().join(&name)
}

fn remove_gpg_extension(p: &Path) -> std::path::PathBuf {
    p.parent()
        .unwrap()
        .join(p.file_stem().unwrap())
        .to_path_buf()
}

fn check_duplicates<'a>(
    args: &Cli,
    duplicates: &'a Vec<&std::path::PathBuf>,
) -> Result<HashMap<&'a std::path::PathBuf, SyncHash>> {
    let mut m = HashMap::new();

    duplicates
        .iter()
        .map(|de| {
            let plain_path = dbg!(args.plain_root.join(&de));
            let mut plain_f = File::open(&plain_path).unwrap();
            let plain_hash = hash_all(&mut plain_f)?;

            let gpg_path = dbg!(add_gpg_extension(&args.gpg_root.join(&de)));
            let mut gpg_f = File::open(&gpg_path).unwrap();
            let gpg_hash = hash_all(&mut Cursor::new(gpg::decrypt(
                &mut gpg_f,
                &args.passphrase.as_bytes(),
            )?))?;

            if plain_hash == gpg_hash {
                m.insert(*de, SyncHash::Agree(plain_hash));
            } else {
                m.insert(*de, SyncHash::Conflict);
            }

            Ok(())
        })
        .collect::<Result<Vec<()>>>()?;

    Ok(m)
}

// struct Revision(u64);

#[derive(Copy, Clone, Debug)]
enum FileStatus {
    Nonexistent,
    Existent(std::time::SystemTime), //, Option<Revision>),
                                     // Conflicted,
}

enum FileChange {
    NoChange(FileStatus),
    Add,
    Mod,
    Del,
}

#[derive(Debug)]
enum SyncAction {
    None,
    PossibleConflict,
    PushPlain,
    DeletePlain,
    PushGpg,
    DeleteGpg,
    //Inconsistency(Box<SyncAction>),
}

fn determine_sync_action(plain: FileChange, gpg: FileChange) -> SyncAction {
    // todo if files were conflicted, deletion or modification of one should not trigger a change to the other
    match (plain, gpg) {
        (FileChange::NoChange(_), FileChange::NoChange(_)) => SyncAction::None,
        (FileChange::NoChange(FileStatus::Nonexistent), FileChange::Add) => SyncAction::PushGpg,
        (FileChange::NoChange(FileStatus::Nonexistent), FileChange::Mod) => SyncAction::PushGpg,
        (FileChange::NoChange(FileStatus::Nonexistent), FileChange::Del) => SyncAction::None,
        (FileChange::NoChange(FileStatus::Existent(_)), FileChange::Add) => {
            SyncAction::PossibleConflict
        }
        (FileChange::NoChange(FileStatus::Existent(_)), FileChange::Mod) => SyncAction::PushGpg,
        (FileChange::NoChange(FileStatus::Existent(_)), FileChange::Del) => SyncAction::DeleteGpg,
        (FileChange::Add, FileChange::NoChange(FileStatus::Nonexistent)) => SyncAction::PushPlain,
        (FileChange::Add, FileChange::NoChange(FileStatus::Existent(_))) => {
            SyncAction::PossibleConflict
        }
        (FileChange::Add, FileChange::Add) => SyncAction::PossibleConflict,
        (FileChange::Add, FileChange::Mod) => SyncAction::PossibleConflict,
        (FileChange::Add, FileChange::Del) => SyncAction::PushPlain, // add wins over del
        (FileChange::Mod, FileChange::NoChange(FileStatus::Nonexistent)) => SyncAction::PushPlain,
        (FileChange::Mod, FileChange::NoChange(FileStatus::Existent(_))) => SyncAction::PushPlain,
        (FileChange::Mod, FileChange::Add) => SyncAction::PossibleConflict,
        (FileChange::Mod, FileChange::Mod) => SyncAction::PossibleConflict,
        (FileChange::Mod, FileChange::Del) => SyncAction::PushPlain, // mod wins over del
        (FileChange::Del, FileChange::NoChange(FileStatus::Nonexistent)) => SyncAction::None,
        (FileChange::Del, FileChange::NoChange(FileStatus::Existent(_))) => SyncAction::DeleteGpg,
        (FileChange::Del, FileChange::Add) => SyncAction::PushGpg, // add wins over del
        (FileChange::Del, FileChange::Mod) => SyncAction::PushPlain, // mod wins over del
        (FileChange::Del, FileChange::Del) => SyncAction::None,
    }
}

fn determine_file_change(prev: FileStatus, cur: FileStatus) -> FileChange {
    match (prev, cur) {
        (FileStatus::Nonexistent, FileStatus::Nonexistent) => {
            FileChange::NoChange(FileStatus::Nonexistent)
        }
        (FileStatus::Nonexistent, FileStatus::Existent(_)) => FileChange::Add,
        (FileStatus::Existent(_), FileStatus::Nonexistent) => FileChange::Del,
        (FileStatus::Existent(t1), FileStatus::Existent(t2)) => {
            if t1 != t2 {
                FileChange::Mod
            } else {
                FileChange::NoChange(FileStatus::Existent(t1))
            }
        }
    }
}

fn file_status(fp: &std::path::PathBuf) -> FileStatus {
    if !fp.exists() {
        return FileStatus::Nonexistent;
    }

    let mtime = std::fs::metadata(&fp).unwrap().modified().unwrap();
    FileStatus::Existent(mtime)
}

fn file_statuses(
    rel_path_without_gpg: &std::path::PathBuf,
    plain_root: &std::path::PathBuf,
    gpg_root: &std::path::PathBuf,
) -> (FileStatus, FileStatus) {
    (
        file_status(&plain_root.join(&rel_path_without_gpg)),
        file_status(&add_gpg_extension(&gpg_root.join(&rel_path_without_gpg))),
    )
}

fn get_file_status_from_db(db: &mut SyncDb, fp: &std::path::PathBuf) -> (FileStatus, FileStatus) {
    db.get(fp)
        .cloned()
        .unwrap_or((FileStatus::Nonexistent, FileStatus::Nonexistent))
}

fn analyze_file_and_update_db(
    db: &mut HashMap<std::path::PathBuf, (FileStatus, FileStatus)>,
    plain_root: &std::path::PathBuf,
    gpg_root: &std::path::PathBuf,
    rel_path_without_gpg: &std::path::PathBuf,
) -> SyncAction {
    let (plain_status_prev, gpg_status_prev) = get_file_status_from_db(db, &rel_path_without_gpg);

    //dbg!(plain_status_prev);
    //dbg!(gpg_status_prev);

    let plain_status_cur = dbg!(file_status(&plain_root.join(&rel_path_without_gpg)));
    let gpg_status_cur = dbg!(file_status(&add_gpg_extension(
        &gpg_root.join(&rel_path_without_gpg)
    )));

    let sync_action = determine_sync_action(
        determine_file_change(plain_status_prev, plain_status_cur),
        determine_file_change(gpg_status_prev, gpg_status_cur),
    );

    db.insert(
        rel_path_without_gpg.clone(),
        (plain_status_cur, gpg_status_cur),
    );

    // persist store db to disk

    sync_action
}

fn check_coincide(
    fp: &std::path::PathBuf,
    plain_root: &std::path::PathBuf,
    gpg_root: &std::path::PathBuf,
    passphrase: &str,
) -> bool {
    let gpg_hash = gpg_file_hash(&add_gpg_extension(&gpg_root.join(fp)), passphrase).unwrap();
    let plain_hash = plain_file_hash(&plain_root.join(fp)).unwrap();
    gpg_hash == plain_hash
}

fn push_plain(
    fp: &std::path::PathBuf,
    passphrase: &str,
    plain_root: &std::path::PathBuf,
    gpg_root: &std::path::PathBuf,
) {
    //    dbg!(&plain_root.join(fp));
    let mut plain_f = File::open(&plain_root.join(fp)).unwrap();
    let gpg_data = gpg::encrypt(&mut plain_f, passphrase.as_bytes()).unwrap();

    //  dbg!(&add_gpg_extension(&gpg_root.join(&fp)));

    //dbg!(&gpg_data);
    let mut gpg_f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&add_gpg_extension(&gpg_root.join(&fp)))
        .unwrap();

    gpg_f.write_all(&gpg_data).unwrap();
}

fn push_gpg(
    fp: &std::path::PathBuf,
    passphrase: &str,
    plain_root: &std::path::PathBuf,
    gpg_root: &std::path::PathBuf,
) {
    let mut gpg_f = File::open(dbg!(&add_gpg_extension(&gpg_root.join(&fp)))).unwrap();
    let plain_data = gpg::decrypt(&mut gpg_f, passphrase.as_bytes()).unwrap();

    let mut plain_f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&plain_root.join(fp))
        .unwrap();
    plain_f.write_all(&plain_data).unwrap();
}

fn perform_sync_action_and_update_db(
    sync_action: SyncAction,
    fp: &std::path::PathBuf,
    db: &mut SyncDb,
    plain_root: &std::path::PathBuf,
    gpg_root: &std::path::PathBuf,
    passphrase: &str,
) {
    match sync_action {
        SyncAction::None => {}
        SyncAction::PossibleConflict => {
            if !check_coincide(fp, plain_root, gpg_root, passphrase) {
                println!("conflict {:?}", fp);
                // todo mark it as conflicted in db
            }
        }
        SyncAction::PushPlain => {
            push_plain(fp, passphrase, plain_root, gpg_root);
        }
        SyncAction::DeletePlain => {
            std::fs::remove_file(plain_root.join(fp)).unwrap();
        }
        SyncAction::PushGpg => {
            push_gpg(fp, passphrase, plain_root, gpg_root);
        }
        SyncAction::DeleteGpg => {
            std::fs::remove_file(&add_gpg_extension(&gpg_root.join(fp))).unwrap();
        }
    }
    db.insert(fp.clone(), file_statuses(fp, plain_root, gpg_root));
}

fn handle_file_change(fp: &std::path::PathBuf) {}

fn is_hidden(p: &std::path::Path) -> bool {
    for os_s in p {
        let s = os_s.to_string_lossy();
        if s.starts_with(".") && s != "." && s != ".." {
            return true;
        }
    }
    false
}

fn main() {
    let args = Cli::from_args();
    validate_args(&args).unwrap();

    // read .gitignore

    let mut plain_files: HashSet<std::path::PathBuf> = HashSet::new();
    visit_dir(&args.plain_root, &mut |de| {
        let relative_path = de
            .path()
            .strip_prefix(&args.plain_root)
            .unwrap()
            .to_path_buf();
        plain_files.insert(relative_path);
    })
    .unwrap();

    let mut gpg_files: HashSet<std::path::PathBuf> = HashSet::new();
    visit_dir(&args.gpg_root, &mut |de| {
        if !is_hidden(&de.path()) {
            if de.path().extension() == Some(OsStr::new("gpg")) {
                let relative_path_without_gpg = remove_gpg_extension(&de.path())
                    .strip_prefix(&args.gpg_root)
                    .unwrap()
                    .to_path_buf();
                gpg_files.insert(relative_path_without_gpg);
            } else {
                println!("In gpg dir, skipping non-.gpg file: {:?}", de)
            }
        } else {
            println!("filtered file {:?}", &de.path());
        }
    })
    .unwrap();

    let mut db = HashMap::new();

    for fp in plain_files.union(&gpg_files) {
        let sync_action =
            analyze_file_and_update_db(&mut db, &args.plain_root, &args.gpg_root, &fp);
        println!("{:?} {:?}", fp, sync_action);

        perform_sync_action_and_update_db(
            sync_action,
            fp,
            &mut db,
            &args.plain_root,
            &args.gpg_root,
            &args.passphrase,
        );
    }

    // todo init watcher even before initial sync!

    let (tx, rx) = channel();

    let mut watcher = watcher(tx, Duration::from_secs(1)).unwrap();

    watcher
        .watch(&args.plain_root, RecursiveMode::Recursive)
        .unwrap();
    watcher
        .watch(&args.gpg_root, RecursiveMode::Recursive)
        .unwrap();

    loop {
        match rx.recv() {
            Ok(event) => {
                //println!("db: {:?}", &db);
                println!("event {:?}", event);
                match event {
                    DebouncedEvent::NoticeWrite(p) | DebouncedEvent::NoticeRemove(p) => {
                        println!("noticed begin of write or remove");
                    }
                    DebouncedEvent::Create(p)
                    | DebouncedEvent::Write(p)
                    | DebouncedEvent::Remove(p) => {
                        let p = p.strip_prefix(std::env::current_dir().unwrap()).unwrap();
                        if !is_hidden(p) {
                            let p = if p.starts_with(&args.plain_root) {
                                // todo if is not ignored plain file
                                p.strip_prefix(&args.plain_root).unwrap().to_path_buf()
                            } else {
                                // todo if is not ignored gpg file
                                remove_gpg_extension(p.strip_prefix(&args.gpg_root).unwrap())
                            };
                            let sync_action = analyze_file_and_update_db(
                                &mut db,
                                &args.plain_root,
                                &args.gpg_root,
                                &p,
                            );
                            println!("{:?} {:?}", &p, sync_action);

                            perform_sync_action_and_update_db(
                                sync_action,
                                &p,
                                &mut db,
                                &args.plain_root,
                                &args.gpg_root,
                                &args.passphrase,
                            );
                        } else {
                            println!("filtered file {:?}", &p);
                        }
                    }
                    DebouncedEvent::Chmod(p) => {
                        println!("chmod");
                    }
                    DebouncedEvent::Rename(p_src, p_dst) => {}
                    DebouncedEvent::Rescan => {}
                    DebouncedEvent::Error(e, po) => {
                        println!("error on path {:?}: {}", po, e);
                    }
                }
            }
            Err(e) => println!("file watch error: {:?}", e),
        }
    }

    // let mut input = String::new();
    // io::stdin().read_line(&mut input).unwrap();
}
