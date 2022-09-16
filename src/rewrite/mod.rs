#![allow(unused, dead_code)]

use anyhow::bail;
use crossbeam_channel::select;

mod diff;
mod fs_ops;
mod fs_utils;
mod gpg;
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
    pub fn init(&mut self) -> anyhow::Result<()> {
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

        // perform initial sync
        self.try_process_events(std::time::Duration::from_millis(10))?;

        Ok(())
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
                        // TODO: ?filter directories or .gpg
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
    ) -> std::io::Result<()> {
        for subpath_of_interest in plain_path_aggregator.iter() {
            println!("plain path touched: {:?}, diffing...", subpath_of_interest);
            diff::TreeReconciler::diff_from_filesystem(
                &self.plain_root,
                &mut self.plain_tree,
                subpath_of_interest,
                diff::TreeType::Plain,
            )?;
        }

        for subpath_of_interest in enc_path_aggregator.iter() {
            println!("enc path touched: {:?}, diffing...", subpath_of_interest);
            diff::TreeReconciler::diff_from_filesystem(
                &self.enc_root,
                &mut self.enc_tree,
                subpath_of_interest,
                diff::TreeType::Encrypted,
            )?;
        }

        Ok(())
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

        // TODO: maybe readd the loop later, it is there to incrementally collect changes in the pathbuf while the initial filesystem ops might still be in progress
        //loop {
        self.mark_all_notified_paths(
            &mut plain_path_aggregator,
            &mut enc_path_aggregator,
            timeout,
        );
        dbg!(&self.plain_tree);
        {
            let paths = std::fs::read_dir(&self.plain_root).unwrap();
            for path in paths {
                println!("Name: {}", path.unwrap().path().display())
            }
        }
        dbg!(&plain_path_aggregator);
        dbg!(&self.enc_tree);
        {
            let paths = std::fs::read_dir(&self.enc_root).unwrap();
            for path in paths {
                println!("Name: {}", path.unwrap().path().display())
            }
        }
        dbg!(&enc_path_aggregator);

        self.diff_with_aggregator(&mut plain_path_aggregator, &mut enc_path_aggregator)?;

        if self.plain_watcher.rx.len() > 0 || self.enc_watcher.rx.len() > 0 {
            println!("new notify events arrived while preparing filesystem diff, discarding and reprocessing...");
            return Ok(());
            // continue;
        }

        let file_ops = merge::calculate_merge(&self.enc_tree, &self.plain_tree);

        if self.plain_watcher.rx.len() > 0 || self.enc_watcher.rx.len() > 0 {
            println!("new notify events arrived while preparing filesystem operations, discarding and reprocessing...");
            return Ok(());
            // continue;
        }

        println!("From now on there must be no additional user caused filesystem accesses! Filesystem notifications during this phase will be ignored. Starting file system modifications");
        dbg!(&file_ops);

        let _plain_watcher_pause_guard = self.plain_watcher.pause_watch();
        let _enc_watcher_pause_guard = self.enc_watcher.pause_watch();

        fs_ops::perform_file_ops(
            &file_ops,
            &self.plain_root,
            &self.enc_root,
            &self.passphrase,
        )?;

        // Reflect the filesystem changes caused by our filesystem operations in the trees.
        update::update_trees_with_changes(&mut self.enc_tree, &mut self.plain_tree, &file_ops);
        // Reflect any deletions performed by the user (that triggered us in the first place) in the trees.
        self.plain_tree.prune_deleted();
        self.enc_tree.prune_deleted();

        self.plain_tree.clean();
        self.enc_tree.clean();

        dbg!(&self.plain_tree);
        {
            let paths = std::fs::read_dir(&self.plain_root).unwrap();
            for path in paths {
                println!("Name: {}", path.unwrap().path().display())
            }
        }
        dbg!(&self.enc_tree);
        {
            let paths = std::fs::read_dir(&self.enc_root).unwrap();
            for path in paths {
                println!("Name: {}", path.unwrap().path().display())
            }
        }

        //}

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

#[cfg(test)]
mod test {

    use super::GpgSync;
    use crate::rewrite::fs_utils;
    use crate::rewrite::gpg;

    use lazy_static::lazy_static;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    fn poll_predicate(p: &mut dyn FnMut() -> bool, timeout: Duration) {
        let start = Instant::now();
        let decrement = Duration::from_millis(5);
        loop {
            if Instant::now() >= start + timeout {
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

    fn init_dir(p: &Path) -> anyhow::Result<()> {
        if p.exists() {
            std::fs::remove_dir_all(&p)?;
        }
        std::fs::create_dir_all(&p)?;

        Ok(())
    }

    fn init_dirs(pr: &Path, gr: &Path) {
        init_dir(pr);
        init_dir(gr);
    }

    fn make_file(p: &Path, s: &[u8]) -> anyhow::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(p)?;
        f.write_all(s)?;

        assert!(p.exists());

        Ok(())
    }

    fn make_encrypted_file(p: &Path, s: &[u8], passphrase: &str) -> anyhow::Result<()> {
        let mut tmpfile = tempfile::NamedTempFile::new()?;
        tmpfile.as_file().write_all(s)?;
        crate::rewrite::fs_ops::encrypt_file(tmpfile.path(), p, passphrase)?;

        Ok(())
    }

    #[test]
    fn test_creation_failure() {
        // TODO one dir is inside the other

        // TODO dir doesn't exist
    }

    #[test]
    fn test_basic() -> anyhow::Result<()> {
        let (pr, gr) = test_roots("test_basic");

        // PD/notes.txt -> GD/notes.txt.gpg
        {
            init_dirs(&pr, &gr);
            make_file(&pr.join("notes.txt"), b"hello")?;
            let mut gpgs = GpgSync::new(&pr, &gr, "passphrase")?;
            gpgs.init()?;

            poll_predicate(
                &mut || {
                    gpgs.try_process_events(std::time::Duration::from_millis(10))
                        .unwrap();

                    gr.join("notes.txt.gpg").exists()
                },
                std::time::Duration::new(2, 0),
            );
        }

        // GD/notes.txt.gpg -> PD/notes.txt
        {
            init_dirs(&pr, &gr);
            make_encrypted_file(&gr.join("notes.txt.gpg"), b"hallo", "passphrase")?;
            let mut gpgs = GpgSync::new(&pr, &gr, "passphrase")?;
            gpgs.init()?;

            poll_predicate(
                &mut || {
                    gpgs.try_process_events(std::time::Duration::from_millis(10))
                        .unwrap();
                    pr.join("notes.txt").exists()
                },
                std::time::Duration::from_secs(2),
            );
        }

        Ok(())
    }

    #[test]
    fn test_directories() -> anyhow::Result<()> {
        let (pr, gr) = test_roots("test_directories");
        // PD/dir/ -> GD/dir/
        {
            init_dirs(&pr, &gr);
            std::fs::create_dir_all(&pr.join("dir"));
            let mut gpgs = GpgSync::new(&pr, &gr, "test")?;
            gpgs.init()?;

            poll_predicate(
                &mut || {
                    gpgs.try_process_events(std::time::Duration::from_millis(10))
                        .unwrap();

                    gr.join("dir").exists()
                },
                std::time::Duration::from_millis(100),
            );
        }

        // GD/dir/ -> PD/dir/
        {
            init_dirs(&pr, &gr);
            std::fs::create_dir_all(&gr.join("dir"));
            let mut gpgs = GpgSync::new(&pr, &gr, "test")?;
            gpgs.init()?;

            poll_predicate(
                &mut || {
                    gpgs.try_process_events(std::time::Duration::from_millis(10))
                        .unwrap();

                    pr.join("dir").exists()
                },
                std::time::Duration::from_millis(100),
            );
        }

        // PD/dir/... -> GD/dir/...
        {
            init_dirs(&pr, &gr);
            std::fs::create_dir_all(&pr.join("dir"));
            make_file(&pr.join("dir").join("notes.txt"), b"hello")?;
            std::fs::create_dir_all(&pr.join("dir").join("dir2"));
            make_file(&pr.join("dir").join("dir2").join("notes2.txt"), b"hello2")?;
            make_file(&pr.join("dir").join("dir2").join("notes3.txt"), b"hello3")?;
            let mut gpgs = GpgSync::new(&pr, &gr, "test")?;
            gpgs.init()?;

            poll_predicate(
                &mut || {
                    gpgs.try_process_events(std::time::Duration::from_millis(10))
                        .unwrap();

                    gr.join("dir").join("notes.txt.gpg").exists()
                        && gr.join("dir").join("dir2").join("notes2.txt.gpg").exists()
                        && gr.join("dir").join("dir2").join("notes3.txt.gpg").exists()
                },
                std::time::Duration::from_millis(100),
            );
        }

        Ok(())
    }

    #[test]
    #[should_panic]
    fn test_wrong_passphrase() {
        let (pr, gr) = test_roots("test_wrong_passphrase");
        init_dirs(&pr, &gr);

        make_encrypted_file(&gr.join("notes.txt.gpg"), b"hallo", "passphrase");
        let mut gpgs = GpgSync::new(&pr, &gr, "test_wrong_passphrase").unwrap();
        gpgs.init().unwrap();
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
    fn test_rename() -> anyhow::Result<()> {
        // TODO check failure when the target file exists
        // TODO check that moving from one directory into the other is not allowed
        let (pr, gr) = test_roots("test_rename");

        init_dirs(&pr, &gr);
        make_file(&pr.join("notes.txt"), b"hello");
        let mut gpgs = GpgSync::new(&pr, &gr, "test")?;
        assert!(!gr.join("notes.txt.gpg").exists());
        gpgs.init()?;
        assert!(gr.join("notes.txt.gpg").exists());
        gpgs.try_process_events(Duration::from_millis(10))?;

        std::fs::rename(pr.join("notes.txt"), pr.join("notes_renamed.txt"))?;
        assert!(!pr.join("notes.txt").exists() && pr.join("notes_renamed.txt").exists());

        poll_predicate(
            &mut || {
                gpgs.try_process_events(Duration::from_millis(200)).unwrap();

                !pr.join("notes.txt").exists()
                    && pr.join("notes_renamed.txt").exists()
                    && !gr.join("notes.txt.gpg").exists()
                    && gr.join("notes_renamed.txt.gpg").exists()
            },
            Duration::from_millis(500),
        );

        Ok(())
    }

    #[test]
    fn test_running_sync() -> anyhow::Result<()> {
        let (pr, gr) = test_roots("test_running_sync");

        init_dirs(&pr, &gr);
        let mut gpgs = GpgSync::new(&pr, &gr, "test")?;

        assert!(!gr.join("notes.txt.gpg").exists());

        make_file(&pr.join("notes.txt"), b"hello");
        poll_predicate(
            &mut || {
                gpgs.try_process_events(Duration::new(0, 200_000_000))
                    .unwrap();

                gr.join("notes.txt.gpg").exists()
            },
            Duration::new(2, 0),
        );

        Ok(())
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
        let gpgs = GpgSync::new(&pr, &gr, "test").unwrap();
        assert!(gr.join("notes.txt.gpg").exists());
        std::mem::drop(gpgs);

        let (_, gr2) = test_roots("test_changed_gpgroot2");
        init_dir(&gr2);
        // pr is already initialized to dir `pr`, trying to connect it to enc dir `gr2` shall fail
        let _gpgs = GpgSync::new(&pr, &gr2, "test").unwrap();
    }

    #[test]
    fn test_conflictcopy_plain() -> anyhow::Result<()> {
        // Test that a conflict copy is also synced back to the other side.

        // also even if their contents are the same, they are conflictcopied. At
        // least for the first run sha1 hashes of the contents should be
        // compared to not create conflicts.
        let (pr, gr) = test_roots("test_conflictcopy_plain");

        init_dirs(&pr, &gr);
        make_file(&pr.join("notes.txt"), b"hello");
        make_encrypted_file(&gr.join("notes.txt.gpg"), b"goodbye", "passphrase")?;

        let mut gpgs = GpgSync::new(&pr, &gr, "passphrase")?;
        gpgs.init()?;
        gpgs.try_process_events(Duration::from_millis(10))?;

        poll_predicate(
            &mut || {
                gpgs.try_process_events(Duration::from_millis(200)).unwrap();

                dbg!(std::fs::read_dir(&pr).unwrap().count()) == 2
                    && dbg!(std::fs::read_dir(&gr).unwrap().count()) == 2
            },
            Duration::from_millis(500),
        );

        Ok(())
    }

    #[test]
    fn test_conflictcopy_enc() -> anyhow::Result<()> {
        // Test that a conflict copy is also synced back to the other side.

        // In the case plain-delete and enc-modified a ConflictCopyEnc is performed, so lets do that.
        let (pr, gr) = test_roots("test_conflictcopy_enc");

        init_dirs(&pr, &gr);
        make_file(&pr.join("notes.txt"), b"hello");

        let mut gpgs = GpgSync::new(&pr, &gr, "passphrase")?;
        gpgs.init()?;
        gpgs.try_process_events(Duration::from_millis(10))?;

        poll_predicate(
            &mut || {
                gpgs.try_process_events(Duration::from_millis(200)).unwrap();

                dbg!(std::fs::read_dir(&pr).unwrap().count()) == 1
                    && dbg!(std::fs::read_dir(&gr).unwrap().count()) == 1
            },
            Duration::from_millis(500),
        );

        std::fs::remove_file(&pr.join("notes.txt"))?;
        make_encrypted_file(&gr.join("notes.txt.gpg"), b"goodbye", "passphrase")?;

        poll_predicate(
            &mut || {
                gpgs.try_process_events(Duration::from_millis(200)).unwrap();

                dbg!(std::fs::read_dir(&pr).unwrap().count()) == 1
                    && dbg!(std::fs::read_dir(&gr).unwrap().count()) == 1
            },
            Duration::from_millis(500),
        );

        Ok(())
    }

    // #[test]
    // fn test_conflictcopy_samecontent_plain() -> anyhow::Result<()> {
    //     // Test that a conflict copy is also synced back to the other side.

    //     // Since their contents are the same, they shall not be conflictcopied. At
    //     // least for the first run sha1 hashes of the contents should be
    //     // compared to not create conflicts.
    //     let (pr, gr) = test_roots("test_conflictcopy_samecontent_plain");

    //     init_dirs(&pr, &gr);
    //     make_file(&pr.join("notes.txt"), b"both_hello");
    //     make_encrypted_file(&gr.join("notes.txt.gpg"), b"both_hello", "passphrase")?;

    //     let mut gpgs = GpgSync::new(&pr, &gr, "passphrase")?;
    //     gpgs.init()?;
    //     gpgs.try_process_events(Duration::from_millis(10))?;

    //     poll_predicate(
    //         &mut || {
    //             gpgs.try_process_events(Duration::from_millis(200)).unwrap();

    //             dbg!(std::fs::read_dir(&pr).unwrap().count()) == 2
    //                 && dbg!(std::fs::read_dir(&gr).unwrap().count()) == 2
    //         },
    //         Duration::from_millis(500),
    //     );

    //     Ok(())
    // }

    // #[test]
    // fn test_conflictcopy_samecontent_enc() -> anyhow::Result<()> {
    //     // Test that a conflict copy is also synced back to the other side.

    //     // Since their contents are the same, they shall not be conflictcopied. At
    //     // least for the first run sha1 hashes of the contents should be
    //     // compared to not create conflicts.
    //     let (pr, gr) = test_roots("test_conflictcopy_samecontent_enc");

    //     init_dirs(&pr, &gr);
    //     make_file(&pr.join("notes.txt"), b"both_hello");
    //     make_encrypted_file(&gr.join("notes.txt.gpg"), b"both_hello", "passphrase")?;

    //     let mut gpgs = GpgSync::new(&pr, &gr, "passphrase")?;
    //     gpgs.init()?;
    //     gpgs.try_process_events(Duration::from_millis(10))?;

    //     poll_predicate(
    //         &mut || {
    //             gpgs.try_process_events(Duration::from_millis(200)).unwrap();

    //             dbg!(std::fs::read_dir(&pr).unwrap().count()) == 2
    //                 && dbg!(std::fs::read_dir(&gr).unwrap().count()) == 2
    //         },
    //         Duration::from_millis(500),
    //     );

    //     Ok(())
    // }
}
