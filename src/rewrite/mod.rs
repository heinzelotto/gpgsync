#![allow(unused, dead_code)]

use anyhow::bail;

mod diff;
mod fs_utils;
mod merge;
mod notifier;
mod tree;
mod update;

// TODO conflictcopy more tests
// TODO DeleteEnc/Plain non-leaf subdir

// TODO test case where a directory is replaced by a file
// TODO test case where a dir is deleted but somethin within it then readded

// TODO if .gpg is added to files in enc dir, test pseude conflict of dir x and file x(.gpg)

// TODO diff from filesystem directory handling, conflict between file and directory

// TODO more diff from filesystem for enc .gpg handling

// TODO ignore non-.enc files in enc dir

// TODO invalid subdir_of_interest, e. g. "." I think problems arise with
// this because we interpret it as a file with name '.' and the fs just
// ignores that path component

// TODO created folder and created child file, ?pathdirt not overwritten

// TODO files in subdir to test recursive diff

// TODO helper function that can be parametrized to handle
// filesystem/tree/result cases more easily.

// TODO test where a/b.txt.gpg is deleted and then instantly after a/b.txt/
// is created (i. e. it is file replaced by dir)

/// The GPGsync instance.
pub struct GpgSync {
    /// The sync database is persisted in the `plain_root` across program runs.
    // db: SyncDb,
    /// Full path where the DB is stored.
    // db_path: PathBuf,
    /// Directory containing all unencrypted files.
    plain_root: std::path::PathBuf,
    /// Directory containing all encrypted files.
    gpg_root: std::path::PathBuf,
    /// Passphrase used for all encryption.
    passphrase: String,
    /// Plain tree
    plain_tree: tree::Tree,
    /// Encrypted tree
    gpg_tree: tree::Tree,
    /// The file watcher.
    plain_watcher: notifier::Notifier,
    /// The file watcher.
    gpg_watcher: notifier::Notifier,
}

impl GpgSync {
    pub fn new(
        plain_root: &std::path::Path,
        gpg_root: &std::path::Path,
        passphrase: &str,
    ) -> anyhow::Result<Self> {
        let plain_watcher = notifier::Notifier::new(&plain_root)?;
        let gpg_watcher = notifier::Notifier::new(&gpg_root)?;

        Ok(GpgSync {
            plain_root: plain_root.to_owned(),
            gpg_root: gpg_root.to_owned(),
            passphrase: passphrase.to_owned(),
            plain_tree: tree::Tree::new(),
            gpg_tree: tree::Tree::new(),
            plain_watcher,
            gpg_watcher,
        })
    }

    pub fn try_process_events(&mut self) -> anyhow::Result<()> {
        // we create a new gpgsync and then, in a loop:
        //
        // retrieve events (and print them to get a feel for them)
        //
        // store only the paths involved in the events in a set
        //
        // after a debounce period of 1.2s of no events, process all entries
        //
        // deduplicate them by sorting and folding, removing paths where a
        // prefixpath is also present
        //
        // call treediff for both trees and respective modified paths
        //
        // repeat another round of treediffs if new events came in in the meantime
        //
        // perform the merge of the trees, keeping the old trees for now
        //
        // repeat redo treediffs if new ones came in. (?maybe also redoing all the
        // tree diffs for paths that are prefixes of newly modified paths)
        //
        // redo the whole merge
        //
        // pray that nothing changes now: perform all file operations and update
        // trees.
        //
        // done. and repeat waiting for new events.

        for plain_event in self.plain_watcher.rx.try_iter() {
            dbg!(plain_event);
            // TODO add to plain path set
        }

        for gpg_event in self.gpg_watcher.rx.try_iter() {
            dbg!(gpg_event);
            // TODO add to enc path set
        }

        // loop {
        //     let received_result = gpgsync.plain_watcher.rx.try_recv();
        //     if let Result::Err(err) = received_result {
        //         match err {
        //             std::sync::mpsc::TryRecvError::Empty => break,
        //             std::sync::mpsc::TryRecvError::Disconnected => return anyhow::bail!("channel unexpectedly disconnected"),
        //         }
        //     }

        //     let received = received_result.ok().unwrap();

        //     dbg!(received);
        // }

        Ok(())
    }
}
