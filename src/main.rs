//use anyhow::{anyhow, Result};
//use md5::{Digest, Md5};
//use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};
//use std::collections::{HashMap, HashSet};
//use std::ffi::OsStr;
//use std::fs::{self, DirEntry, File};
//use std::io::{self, Cursor, Read, Write};
use std::path::PathBuf;
//use std::sync::mpsc::channel; // todo crossbeam
// use std::str::from_utf8;
//use std::time::Duration;
use structopt::StructOpt;

//use serde::{Deserialize, Serialize};

mod gpg;

mod gpgsync;
mod syncdb;
mod syncentity;

mod filesync;

#[derive(StructOpt)]
struct Cli {
    /// The plaintext data path
    #[structopt(parse(from_os_str))]
    plain_root: PathBuf,
    /// The encrypted gpg path
    #[structopt(parse(from_os_str))]
    gpg_root: PathBuf,
    /// The passphrase
    passphrase: String,
}

fn main() {
    let args = Cli::from_args();

    let mut gpg_sync =
        gpgsync::GpgSync::new(&args.plain_root, &args.gpg_root, &args.passphrase).unwrap();

    loop {
        gpg_sync.process_events();
    }
}
