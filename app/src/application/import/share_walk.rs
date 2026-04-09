use super::inner::RawFile;

pub(super) struct DirectoryEntries<T> {
    pub(super) child_dirs: Vec<(T, String)>,
    pub(super) raw_files: Vec<RawFile>,
}

impl<T> DirectoryEntries<T> {
    pub(super) fn new(child_dirs: Vec<(T, String)>, raw_files: Vec<RawFile>) -> Self {
        Self {
            child_dirs,
            raw_files,
        }
    }
}

pub(super) struct ShareTraversal<T> {
    pending_dirs: Vec<(T, String)>,
    raw_files: Vec<RawFile>,
}

impl<T> ShareTraversal<T> {
    pub(super) fn new(root: (T, String)) -> Self {
        Self {
            pending_dirs: vec![root],
            raw_files: Vec::new(),
        }
    }

    pub(super) fn next_dir(&mut self) -> Option<(T, String)> {
        self.pending_dirs.pop()
    }

    pub(super) fn extend(&mut self, entries: DirectoryEntries<T>) {
        self.pending_dirs.extend(entries.child_dirs);
        self.raw_files.extend(entries.raw_files);
    }

    pub(super) fn into_raw_files(self) -> Vec<RawFile> {
        self.raw_files
    }
}

pub(super) fn child_share_path(parent_path: &str, name: &str) -> String {
    format!("{}/{}", parent_path, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::import::inner::Etag;

    #[test]
    fn share_traversal_collects_nested_entries() {
        let mut traversal = ShareTraversal::new((0, "/root".to_string()));
        traversal.extend(DirectoryEntries::new(
            vec![(1, "/root/Season 01".into())],
            vec![RawFile {
                id: Some(2),
                name: "movie.mkv".into(),
                etag: Etag::Md5("etag".into()),
                size: 100,
                path: "/root".into(),
            }],
        ));

        assert_eq!(traversal.next_dir(), Some((1, "/root/Season 01".into())));
        assert_eq!(traversal.into_raw_files().len(), 1);
    }
}
