//use anyhow::{anyhow, Result};
//use std::ffi::OsStr;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum FileStatus {
    Nonexistent,
    Existent(std::time::SystemTime), //, Option<Revision>),
                                     // Conflicted,
}

pub enum FileChange {
    NoChange(FileStatus),
    Add,
    Mod,
    Del,
}

#[derive(Debug)]
pub enum SyncAction {
    None,
    PossibleConflict,
    PushPlain,
    DeletePlain,
    PushGpg,
    DeleteGpg,
    //Inconsistency(Box<SyncAction>),
}

pub fn determine_sync_action(plain: FileChange, gpg: FileChange) -> SyncAction {
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
        (FileChange::NoChange(FileStatus::Existent(_)), FileChange::Del) => SyncAction::DeletePlain,
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

pub fn determine_file_change(prev: FileStatus, cur: FileStatus) -> FileChange {
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
