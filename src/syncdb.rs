use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;

//use serde::{Deserialize, Serialize};

use crate::filesync::*;
use crate::syncentity::*;

pub struct SyncDb {
    db: HashMap<PathBuf, (FileStatus, FileStatus)>,
}

impl SyncDb {
    // TODO: remove [Nonexistent, Nonexistent] entries from the db to remove any remnants of the file names

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

        let serialized = serde_json::to_string(&self.db).unwrap();

        f.write_all(&serialized.as_bytes()).unwrap();
    }

    pub fn load_db(fp: &PathBuf) -> Self {
        // TODO also read gpg_path from disk and refuse to load if existing db is for a different sync target
        // TODO this function would then load "existing sync configuration", not just the db

        match File::open(fp) {
            Ok(mut f) => {
                println!("loading existing db from {:?}", fp);

                let mut s = String::new();
                f.read_to_string(&mut s).unwrap();
                dbg!(&s);
                let deserialized = serde_json::from_str(&s).unwrap();

                SyncDb { db: deserialized }
            }
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                println!("no db found yet at {:?}", fp);
                SyncDb { db: HashMap::new() }
            }
            Err(e) => panic!("{:?}", e),
        }
    }
}
