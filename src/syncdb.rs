use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::filesync::FileStatus;
use crate::syncentity::SyncEntity;

const DB_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct SyncDb {
    gpg_root: PathBuf,
    db: HashMap<PathBuf, (FileStatus, FileStatus)>,
    db_version: u32,
}

impl SyncDb {
    // TODO: remove [Nonexistent, Nonexistent] entries from the db to remove any remnants of the file names

    pub fn new(gpg_root: &Path) -> Self {
        Self {
            gpg_root: gpg_root.to_owned(),
            db: HashMap::new(),
            db_version: DB_VERSION,
        }
    }

    pub fn get_file_status(&self, se: &SyncEntity) -> (FileStatus, FileStatus) {
        self.db
            .get(se.rel_without_gpg())
            .cloned()
            .unwrap_or((FileStatus::Nonexistent, FileStatus::Nonexistent))
    }
    pub fn set_file_status(
        &mut self,
        se: &SyncEntity,
        plain_status: FileStatus,
        gpg_status: FileStatus,
    ) {
        self.db
            .insert(se.rel_without_gpg().clone(), (plain_status, gpg_status));
    }
    pub fn save_db(&self, fp: &PathBuf) {
        // TODO also persist gpg_path to disk to make sure that the database is for the correct sync target

        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&fp)
            .unwrap();

        let serialized = serde_json::to_string(&self).unwrap();

        f.write_all(&serialized.as_bytes()).unwrap();
    }

    pub fn load_db(fp: &PathBuf) -> Option<Self> {
        // TODO also read gpg_path from disk and refuse to load if existing db is for a different sync target
        // TODO this function would then load "existing sync configuration", not just the db

        match File::open(fp) {
            Ok(mut f) => {
                println!("loading existing db from {:?}", fp);

                let mut s = String::new();
                f.read_to_string(&mut s).unwrap();
                dbg!(&s);
                let deserialized: SyncDb = serde_json::from_str(&s).unwrap();

                // make sure the db schema is correct
                assert!(deserialized.db_version == DB_VERSION);

                Some(deserialized)
            }
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                println!("no db found yet at {:?}", fp);
                None
            }
            Err(e) => panic!("{:?}", e),
        }
    }

    pub fn gpg_root(&self) -> &Path {
        &self.gpg_root
    }
}
