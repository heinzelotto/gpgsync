#[derive(Debug)]
pub struct PathAggregator(Vec<std::path::PathBuf>);

impl PathAggregator {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Adds a path to the sorted vec. Then the vec is folded in such a way that
    /// all paths that are children of other paths in the vec are removed. Only
    /// the topmost ancestor path will remain.
    pub fn mark_path(&mut self, p: &std::path::Path) {
        match self.0.binary_search_by(|pb| pb.as_path().cmp(p)) {
            Ok(_) => return,
            Err(idx) => {
                self.0.insert(idx, p.to_owned());

                // find ancestor paths
                let first_ancestor_idx = self.0[..idx]
                    .binary_search_by(|pb| {
                        if p.starts_with(pb) {
                            std::cmp::Ordering::Greater
                        } else {
                            std::cmp::Ordering::Less
                        }
                    })
                    .unwrap_err();

                // find children paths
                let first_nonchild_offset = self.0[idx + 1..]
                    .binary_search_by(|pb| {
                        if pb.starts_with(p) {
                            std::cmp::Ordering::Less
                        } else {
                            std::cmp::Ordering::Greater
                        }
                    })
                    .unwrap_err();

                if first_ancestor_idx + 1 < idx + 1 + first_nonchild_offset {
                    self.0
                        .drain(first_ancestor_idx + 1..idx + 1 + first_nonchild_offset);
                }
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &std::path::Path> {
        self.0.iter().map(|p| p.as_path())
    }
}

#[cfg(test)]
mod test {
    use super::PathAggregator;
    use std::path::Path;

    #[test]
    fn test_path_aggregator_1() -> anyhow::Result<()> {
        // parent dir first

        let mut pa = PathAggregator::new();

        let p1 = Path::new("/dir1/subdir1");
        let p2 = Path::new("/dir1/subdir1/f1.txt");
        let p3 = Path::new("/dir1/subdir2/g1.txt");

        pa.mark_path(p1);
        pa.mark_path(p2);
        pa.mark_path(p3);

        assert_eq!(
            pa.iter().collect::<Vec<&std::path::Path>>(),
            vec![
                Path::new("/dir1/subdir1/"),
                Path::new("/dir1/subdir2/g1.txt"),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_path_aggregator_2() -> anyhow::Result<()> {
        // parent dir last

        let mut pa = PathAggregator::new();

        let p1 = Path::new("/dir1/subdir1/f1.txt");
        let p2 = Path::new("/dir1/subdir1");
        let p3 = Path::new("/dir1/subdir2/g1.txt");

        pa.mark_path(p1);
        pa.mark_path(p2);
        pa.mark_path(p3);

        assert_eq!(
            pa.iter().collect::<Vec<&std::path::Path>>(),
            vec![
                Path::new("/dir1/subdir1/"),
                Path::new("/dir1/subdir2/g1.txt"),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_path_aggregator_3() -> anyhow::Result<()> {
        // parent dir last

        let mut pa = PathAggregator::new();

        let p1 = Path::new("/dir1/subdir2/g1.txt");
        let p2 = Path::new("/dir1/subdir1/f1.txt");
        let p3 = Path::new("/");
        let p4 = Path::new("/dir1/subdir1");

        pa.mark_path(p1);
        pa.mark_path(p2);
        pa.mark_path(p3);

        assert_eq!(
            pa.iter().collect::<Vec<&std::path::Path>>(),
            vec![Path::new("/"),]
        );

        Ok(())
    }
}
