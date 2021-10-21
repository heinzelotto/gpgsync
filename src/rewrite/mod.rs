#![allow(unused, dead_code)]

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
