use super::{
    LibraryFile, Pan115FileEntry, Pan189File, Pan189Folder,
    inner::RawFile,
    share_walk::{DirectoryEntries, child_share_path},
};

pub(super) fn collect_pan123_directory_entries(
    files: &[LibraryFile],
    parent_path: &str,
) -> DirectoryEntries<i64> {
    let mut child_dirs = Vec::new();
    let mut raw_files = Vec::new();

    for file in files {
        if file.is_dir {
            child_dirs.push((file.file_id, child_share_path(parent_path, &file.file_name)));
        } else {
            raw_files.push(RawFile {
                id: Some(file.file_id),
                name: file.file_name.to_owned(),
                etag: file.etag.as_str().into(),
                size: file.size,
                path: parent_path.to_owned(),
            });
        }
    }

    DirectoryEntries::new(child_dirs, raw_files)
}

pub(super) fn collect_pan189_directory_entries(
    folders: &[Pan189Folder],
    files: &[Pan189File],
    parent_path: &str,
) -> DirectoryEntries<String> {
    let child_dirs = folders
        .iter()
        .map(|folder| {
            (
                folder.id.to_owned(),
                child_share_path(parent_path, &folder.name),
            )
        })
        .collect();

    let raw_files = files
        .iter()
        .map(|file| RawFile {
            id: None,
            name: file.name.to_owned(),
            etag: file.md5.as_str().into(),
            size: file.size,
            path: parent_path.to_owned(),
        })
        .collect();

    DirectoryEntries::new(child_dirs, raw_files)
}

pub(super) fn collect_pan115_directory_entries(
    entries: &[Pan115FileEntry],
    parent_path: &str,
) -> DirectoryEntries<String> {
    let mut child_dirs = Vec::new();
    let mut raw_files = Vec::new();

    for entry in entries {
        if entry.is_file() {
            raw_files.push(RawFile {
                id: None,
                name: entry.name.to_owned(),
                etag: entry.sha.as_deref().unwrap_or_default().into(),
                size: entry.size,
                path: parent_path.to_owned(),
            });
        } else if let Some(cid) = &entry.cid {
            child_dirs.push((cid.to_owned(), child_share_path(parent_path, &entry.name)));
        }
    }

    DirectoryEntries::new(child_dirs, raw_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::import::inner::Etag;

    #[test]
    fn collect_pan123_directory_entries_splits_dirs_and_files() {
        let files = vec![
            LibraryFile {
                file_id: 1,
                file_name: "Season 01".into(),
                is_dir: true,
                size: 0,
                etag: String::new(),
            },
            LibraryFile {
                file_id: 2,
                file_name: "movie.mkv".into(),
                is_dir: false,
                size: 100,
                etag: "etag".into(),
            },
        ];

        let entries = collect_pan123_directory_entries(&files, "/root");

        assert_eq!(entries.child_dirs, vec![(1, "/root/Season 01".into())]);
        assert_eq!(entries.raw_files.len(), 1);
        assert_eq!(entries.raw_files[0].name, "movie.mkv");
    }

    #[test]
    fn collect_pan189_directory_entries_tracks_folders_and_files() {
        let folders = vec![Pan189Folder {
            id: "next".into(),
            name: "folder".into(),
        }];
        let files = vec![Pan189File {
            name: "episode.mkv".into(),
            size: 200,
            md5: "md5".into(),
        }];

        let entries = collect_pan189_directory_entries(&folders, &files, "/parent");

        assert_eq!(
            entries.child_dirs,
            vec![("next".into(), "/parent/folder".into())]
        );
        assert_eq!(entries.raw_files.len(), 1);
        assert!(matches!(&entries.raw_files[0].etag, Etag::Md5(value) if value == "md5"));
    }

    #[test]
    fn collect_pan115_directory_entries_tracks_cids_and_files() {
        let entries = vec![
            Pan115FileEntry {
                cid: Some("child".into()),
                fid: None,
                name: "dir".into(),
                size: 0,
                sha: None,
            },
            Pan115FileEntry {
                cid: None,
                fid: Some("file".into()),
                name: "subtitle.srt".into(),
                size: 50,
                sha: Some("sha1".into()),
            },
        ];

        let collected = collect_pan115_directory_entries(&entries, "/root");

        assert_eq!(
            collected.child_dirs,
            vec![("child".into(), "/root/dir".into())]
        );
        assert_eq!(collected.raw_files.len(), 1);
        assert_eq!(collected.raw_files[0].name, "subtitle.srt");
    }
}
