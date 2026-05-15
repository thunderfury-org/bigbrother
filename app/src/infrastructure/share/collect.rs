use crate::{
    domain::share::RawFile,
    infrastructure::client::{pan115, pan123, pan189, quark},
};

use super::traversal::{DirectoryEntries, child_share_path};

pub(crate) fn collect_pan123_directory_entries(
    files: &[pan123::File],
    parent_path: &str,
) -> DirectoryEntries<i64> {
    let mut child_dirs = Vec::new();
    let mut raw_files = Vec::new();

    for file in files {
        if file.is_dir() {
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

pub(crate) fn collect_pan189_directory_entries(
    folders: &[pan189::Folder],
    files: &[pan189::File],
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

pub(crate) fn collect_pan115_directory_entries(
    entries: &[pan115::FileEntry],
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

pub(crate) fn collect_quark_directory_entries(
    folders: &[quark::Folder],
    files: &[quark::File],
    parent_path: &str,
) -> DirectoryEntries<String> {
    let child_dirs = folders
        .iter()
        .map(|folder| {
            (
                folder.fid.to_owned(),
                child_share_path(parent_path, &folder.file_name),
            )
        })
        .collect();

    let raw_files = files
        .iter()
        .map(|file| RawFile {
            id: None,
            name: file.file_name.to_owned(),
            etag: "".into(),
            size: file.size,
            path: parent_path.to_owned(),
        })
        .collect();

    DirectoryEntries::new(child_dirs, raw_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::share::Etag;

    #[test]
    fn collect_pan123_directory_entries_splits_dirs_and_files() {
        let files = vec![
            pan123::File {
                file_id: 1,
                file_name: "Season 01".into(),
                file_type: 1,
                size: 0,
                _created_at: time::OffsetDateTime::UNIX_EPOCH,
                _updated_at: time::OffsetDateTime::UNIX_EPOCH,
                etag: String::new(),
                abs_path: String::new(),
            },
            pan123::File {
                file_id: 2,
                file_name: "movie.mkv".into(),
                file_type: 0,
                size: 100,
                _created_at: time::OffsetDateTime::UNIX_EPOCH,
                _updated_at: time::OffsetDateTime::UNIX_EPOCH,
                etag: "etag".into(),
                abs_path: String::new(),
            },
        ];

        let entries = collect_pan123_directory_entries(&files, "/root");

        assert_eq!(entries.child_dirs, vec![(1, "/root/Season 01".into())]);
        assert_eq!(entries.raw_files.len(), 1);
        assert_eq!(entries.raw_files[0].name, "movie.mkv");
    }

    #[test]
    fn collect_pan189_directory_entries_tracks_folders_and_files() {
        let folders = vec![pan189::Folder {
            id: "next".into(),
            name: "folder".into(),
        }];
        let files = vec![pan189::File {
            id: "file".into(),
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
            pan115::FileEntry {
                cid: Some("child".into()),
                fid: None,
                name: "dir".into(),
                size: 0,
                sha: None,
            },
            pan115::FileEntry {
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
