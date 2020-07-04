use anyhow::{anyhow, Result};
use md5::{Digest, Md5};
//use notify::*;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{self, metadata, DirEntry, File};
use std::io::{self, Cursor, Read, Write};
use std::path::Path;
// use std::str::from_utf8;
use structopt::StructOpt;

mod gpg;

#[derive(StructOpt)]
struct Cli {
    /// The plaintext data path
    #[structopt(parse(from_os_str))]
    plain_path: std::path::PathBuf, // plain_base_path
    /// The encrypted gpg path
    #[structopt(parse(from_os_str))]
    gpg_path: std::path::PathBuf, // gpg_base_path
    /// The passphrase
    passphrase: String,
}

fn validate_args(args: &Cli) -> Result<()> {
    if args.plain_path.starts_with(&args.gpg_path) || args.gpg_path.starts_with(&args.plain_path) {
        return Err(anyhow!("The two paths must not contain each other."));
    }

    if !args.plain_path.exists() {
        return Err(anyhow!(format!("No such directory: {:?}", args.plain_path)));
    }
    if !args.gpg_path.exists() {
        return Err(anyhow!(format!("No such directory: {:?}", args.gpg_path)));
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

fn plain_file_hash(p:&Path) -> io::Result<Vec<u8>> {
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

fn check_duplicates<'a>(
    args: &Cli,
    duplicates: &'a Vec<&std::path::PathBuf>,
) -> Result<HashMap<&'a std::path::PathBuf, SyncHash>> {
    let mut m = HashMap::new();

    duplicates
        .iter()
        .map(|de| {
            let plain_path = dbg!(args.plain_path.join(&de));
            let mut plain_f = File::open(&plain_path).unwrap();
            let plain_hash = hash_all(&mut plain_f)?;

            let gpg_path = dbg!(add_gpg_extension(&args.gpg_path.join(&de)));
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

#[derive(Copy, Clone)]
enum FileStatus {
    Nonexistent,
    Existent(std::time::SystemTime), //, Option<Revision>),
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

fn get_file_status_from_db(
    db: &mut HashMap<std::path::PathBuf, (FileStatus, FileStatus)>,
    fp: &std::path::PathBuf,
) -> (FileStatus, FileStatus) {
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

    let plain_status_cur = file_status(&plain_root.join(&rel_path_without_gpg));
    let gpg_status_cur = file_status(&add_gpg_extension(&gpg_root.join(&rel_path_without_gpg)));

    let sync_action = determine_sync_action(
        determine_file_change(plain_status_prev, plain_status_cur),
        determine_file_change(gpg_status_prev, gpg_status_cur),
    );

    db.insert(
        rel_path_without_gpg.clone(),
        (plain_status_cur, gpg_status_cur),
    );

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

fn push_plain(fp: &std::path::PathBuf, passphrase: &str, plain_root: &std::path::PathBuf, gpg_root: &std::path::PathBuf) {
    let mut plain_f = File::open(&plain_root.join(fp)).unwrap();
    let gpg_data = gpg::encrypt(&mut plain_f, passphrase.as_bytes()).unwrap();

    let mut gpg_f = std::fs::OpenOptions::new().write(true).create(true).open(&add_gpg_extension(&gpg_root.join(&fp))).unwrap();

    gpg_f.write_all(&gpg_data);
}

fn push_gpg(fp: &std::path::PathBuf, passphrase: &str, plain_root: &std::path::PathBuf, gpg_root: &std::path::PathBuf) {
    let mut gpg_f = File::open(dbg!(&add_gpg_extension(&gpg_root.join(&fp)))).unwrap();
    let plain_data = gpg::decrypt(&mut gpg_f, passphrase.as_bytes()).unwrap();

    let mut plain_f = std::fs::OpenOptions::new().write(true).create(true).open(&plain_root.join(fp)).unwrap();
    plain_f.write_all(&plain_data);
}

fn main() {
    let args = Cli::from_args();
    validate_args(&args).unwrap();

    // read .gitignore

    let mut plain_files: HashSet<std::path::PathBuf> = HashSet::new();
    visit_dir(&args.plain_path, &mut |de| {
        let relative_path = de
            .path()
            .strip_prefix(&args.plain_path)
            .unwrap()
            .to_path_buf();
        plain_files.insert(relative_path);
    })
    .unwrap();

    let mut gpg_files: HashSet<std::path::PathBuf> = HashSet::new();
    visit_dir(&args.gpg_path, &mut |de| {
        if de.path().extension() == Some(OsStr::new("gpg")) {
            let relative_path_without_gpg = de
                .path()
                .parent()
                .unwrap()
                .join(de.path().file_stem().unwrap())
                .strip_prefix(&args.gpg_path)
                .unwrap()
                .to_path_buf();
            gpg_files.insert(relative_path_without_gpg);
        } else {
            println!("In gpg dir, skipping non-.gpg file: {:?}", de)
        }
    })
    .unwrap();

    let mut db = HashMap::new();

    for fp in plain_files.union(&gpg_files) {
        let sync_action =
            analyze_file_and_update_db(&mut db, &args.plain_path, &args.gpg_path, &fp);
        println!("{:?} {:?}", fp, sync_action);

        match sync_action {
            SyncAction::None => {}
            SyncAction::PossibleConflict => {
                if !check_coincide(fp, &args.plain_path, &args.gpg_path, &args.passphrase) {
                    println!("conflict {:?}", fp);
                }
            }
            SyncAction::PushPlain => {
                push_plain(fp, &args.passphrase, &args.plain_path, &args.gpg_path);
            }
            SyncAction::DeletePlain => {
                std::fs::remove_file(&args.plain_path.join(fp)).unwrap();
            }
            SyncAction::PushGpg => {
                push_gpg(fp, &args.passphrase, &args.plain_path, &args.gpg_path);
            }
            SyncAction::DeleteGpg => {
                std::fs::remove_file(&add_gpg_extension(&args.plain_path.join(fp))).unwrap();
            }
        }
    }

    // let diff = compare_sets(&plain_files, &gpg_files);

    // let mut sync_map = HashMap::new();

    // let duplicates_map = check_duplicates(&args, &diff.both).unwrap();

    // println!("differences between files:\n{:#?}", diff);

    // let mut conflicts = Vec::new();
    // duplicates_map.iter().for_each(|(p, sh)| match sh {
    //     SyncHash::Agree(hash) => {
    //         sync_map.insert(p, hash);
    //     }
    //     SyncHash::Conflict => {
    //         conflicts.push(p);
    //     }
    // });

    // println!("hashmap so far:\n{:?}", sync_map);
    // println!("conflicts:\n{:?}", &conflicts);
    // if !conflicts.is_empty() {
    //     panic!("conflicts detected");
    // }

    // let mut file_hashes: HashMap<std::path::PathBuf, Vec<u8>> = HashMap::new();
    // visit_dir(&args.path, &mut |de| {
    //     let mut f = File::open(de.path()).unwrap();
    //     let relative_path = de.path().strip_prefix(&args.path).unwrap().to_path_buf();
    //     file_hashes.insert(relative_path, hash_all(&mut f).unwrap());
    // })
    // .unwrap();
    // println!("\nFiles inside directory {:?}:", args.path);
    // for (a, b) in &file_hashes {
    //     println!("{:?} {:x?}", a, b);
    // }

    // println!("\nGpg files inside directory {:?}:", args.gpg_path);
    // let mut gpg_file_hashes: HashMap<std::path::PathBuf, Vec<u8>> = HashMap::new();
    // visit_dir(&args.gpg_path, &mut |f| {
    //     if f.path().extension() == Some(OsStr::new("gpg")) {
    //         let relative_path_without_gpg = f
    //             .path()
    //             .parent()
    //             .unwrap()
    //             .join(f.path().file_stem().unwrap())
    //             .strip_prefix(&args.gpg_path)
    //             .unwrap()
    //             .to_path_buf();
    //         let hash = gpg_file_hash(&f.path(), &args.passphrase).unwrap();
    //         gpg_file_hashes.insert(relative_path_without_gpg, hash);
    //     } else {
    //         println!("Skipping non-.gpg file {:?}", f)
    //     }
    // })
    // .unwrap();

    // for (a, b) in &gpg_file_hashes {
    //     println!("{:?} {:x?}", a, b);
    // }

    // compare_sets(&file_hashes, &gpg_file_hashes);

    // ignore non-.gpg files in the gpg dir

    // let mut watcher: notify::RecommendedWatcher = Watcher::new_immediate(|res| match res {
    //     Ok(event) => println!("event: {:?}", event),
    //     Err(e) => println!("watch error: {:?}", e),
    // })
    // .unwrap();

    // watcher.watch(args.path, RecursiveMode::Recursive).unwrap();
    // watcher
    //     .watch(args.gpg_path, RecursiveMode::Recursive)
    //     .unwrap();

    // let mut input = String::new();
    // io::stdin().read_line(&mut input).unwrap();
}
