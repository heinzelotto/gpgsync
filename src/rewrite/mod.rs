#![allow(unused, dead_code)]

use anyhow::bail;
use crossbeam_channel::select;

mod diff;
mod fs_utils;
mod merge;
mod notifier;
mod path_aggregator;
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

// TODO: ?will a FileOperation::Encryption() work with a directory where nothing
// needs to be encrypted

/// The GPGsync instance.
pub struct GpgSync {
    /// The sync database is persisted in the `plain_root` across program runs.
    // db: SyncDb,
    /// Full path where the DB is stored.
    // db_path: PathBuf,
    /// Directory containing all unencrypted files.
    plain_root: std::path::PathBuf,
    /// Directory containing all encrypted files.
    enc_root: std::path::PathBuf,
    /// Passphrase used for all encryption.
    passphrase: String,
    /// Plain tree
    plain_tree: tree::Tree,
    /// Encrypted tree
    enc_tree: tree::Tree,
    /// The file watcher.
    plain_watcher: notifier::Notifier,
    /// The file watcher.
    enc_watcher: notifier::Notifier,
}

impl GpgSync {
    pub fn new(
        plain_root: &std::path::Path,
        enc_root: &std::path::Path,
        passphrase: &str,
    ) -> anyhow::Result<Self> {
        let plain_root = if plain_root.is_absolute() {
            plain_root.to_owned()
        } else {
            let mut cwd = std::env::current_dir().unwrap();
            cwd.push(plain_root);
            cwd
        };
        let enc_root = if enc_root.is_absolute() {
            enc_root.to_owned()
        } else {
            let mut cwd = std::env::current_dir().unwrap();
            cwd.push(enc_root);
            cwd
        };

        let plain_watcher = notifier::Notifier::new(&plain_root)?;
        let enc_watcher = notifier::Notifier::new(&enc_root)?;

        Ok(GpgSync {
            plain_root,
            enc_root,
            passphrase: passphrase.to_owned(),
            plain_tree: tree::Tree::new(),
            enc_tree: tree::Tree::new(),
            plain_watcher,
            enc_watcher,
        })
    }

    // TODO:
    //pub fn init_from_savefile_or_fs() TODO e. g. currently it crashes when no
    // init is done, tree is empty, and a deleted path is notified which then
    // neither exists in the tree nor in the fs
    pub fn init(&mut self) {
        // as long as we don't support a persistent database, just compare the
        // initially empty tree with the directory every time
        diff::TreeReconciler::diff_from_filesystem(
            &self.plain_root,
            &mut self.plain_tree,
            std::path::Path::new(""),
            diff::TreeType::Plain,
        );

        diff::TreeReconciler::diff_from_filesystem(
            &self.enc_root,
            &mut self.enc_tree,
            std::path::Path::new(""),
            diff::TreeType::Encrypted,
        );
    }

    /// Drain all notified paths and feed them to the aggregators.
    fn mark_all_notified_paths(
        &mut self,
        plain_path_aggregator: &mut path_aggregator::PathAggregator,
        enc_path_aggregator: &mut path_aggregator::PathAggregator,
        timeout: std::time::Duration,
    ) {
        while crossbeam_channel::select! {
            recv(self.plain_watcher.rx) -> msg => {
                dbg!(&msg);
                if let Ok(ev) = msg {
                    for p in ev.paths {
                        plain_path_aggregator.mark_path(p.strip_prefix(&self.plain_root).unwrap());
                    }
                }
                true
            },
            recv(self.enc_watcher.rx) -> msg => {
                dbg!(&msg);
                if let Ok(ev) = msg {
                    for p in ev.paths {
                        // TODO: ?filter directoryes or .gpg
                        enc_path_aggregator.mark_path(p.strip_prefix(&self.enc_root).unwrap());
                    }
                }
                true
                },
            default(timeout) => {println!("no event received for a while..."); false},
        } {}
    }

    /// Diff the trees with all the paths from a path aggregator.
    fn diff_with_aggregator(
        &mut self,
        plain_path_aggregator: &mut path_aggregator::PathAggregator,
        enc_path_aggregator: &mut path_aggregator::PathAggregator,
    ) {
        for subpath_of_interest in plain_path_aggregator.iter() {
            println!("plain path touched: {:?}, diffing...", subpath_of_interest);
            diff::TreeReconciler::diff_from_filesystem(
                &self.plain_root,
                &mut self.plain_tree,
                subpath_of_interest,
                diff::TreeType::Plain,
            );
        }

        for subpath_of_interest in enc_path_aggregator.iter() {
            println!("enc path touched: {:?}, diffing...", subpath_of_interest);
            diff::TreeReconciler::diff_from_filesystem(
                &self.plain_root,
                &mut self.plain_tree,
                subpath_of_interest,
                diff::TreeType::Encrypted,
            );
        }
    }

    // we create a new gpgsync and then, in a loop (this function is this loop):
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
    pub fn try_process_events(&mut self, timeout: std::time::Duration) -> anyhow::Result<()> {
        let mut plain_path_aggregator = path_aggregator::PathAggregator::new();
        let mut enc_path_aggregator = path_aggregator::PathAggregator::new();

        loop {
            self.mark_all_notified_paths(
                &mut plain_path_aggregator,
                &mut enc_path_aggregator,
                timeout,
            );
            self.diff_with_aggregator(&mut plain_path_aggregator, &mut enc_path_aggregator);

            if self.plain_watcher.rx.len() > 0 || self.enc_watcher.rx.len() > 0 {
                println!("new notify events arrived while preparing filesystem diff, discarding and reprocessing...");
                continue;
            }

            dbg!(&self.plain_tree);
            dbg!(&self.enc_tree);

            let file_ops = merge::calculate_merge(&self.enc_tree, &self.plain_tree);

            if self.plain_watcher.rx.len() > 0 || self.enc_watcher.rx.len() > 0 {
                println!("new notify events arrived while preparing filesystem operations, discarding and reprocessing...");
                continue;
            }

            println!("From now on there must be no additional user caused filesystem accesses. Starting file system modifications");
            dbg!(&file_ops);

            // TODO: perform fs ops

            update::update_trees_with_changes(&mut self.enc_tree, &mut self.plain_tree, &file_ops);
        }

        // for plain_event in self.plain_watcher.rx.try_iter() {
        //     dbg!(plain_event);
        //     // TODO add to plain path set
        // }

        // for gpg_event in self.gpg_watcher.rx.try_iter() {
        //     dbg!(gpg_event);
        //     // TODO add to enc path set
        // }

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
