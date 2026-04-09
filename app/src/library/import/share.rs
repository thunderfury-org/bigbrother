use reqwest::Url;
use tracing::info;

pub use super::source::ShareUrl;

use super::{
    ImportedMedia, Importer, LibraryFile, Pan115FileEntry, Pan189File, Pan189Folder,
    inner::{MediaFile, RawFile},
    source::{parse_pan115_share_parts, parse_pan123_share_parts, parse_pan189_share_code},
};
use crate::application::import_ports::{LibraryGateway, MetadataCatalog, ShareSource};
use crate::error::{AppError, AppResult};

struct DirectoryEntries<T> {
    child_dirs: Vec<(T, String)>,
    raw_files: Vec<RawFile>,
}

impl<T> DirectoryEntries<T> {
    fn new(child_dirs: Vec<(T, String)>, raw_files: Vec<RawFile>) -> Self {
        Self {
            child_dirs,
            raw_files,
        }
    }
}

struct ShareTraversal<T> {
    pending_dirs: Vec<(T, String)>,
    raw_files: Vec<RawFile>,
}

impl<T> ShareTraversal<T> {
    fn new(root: (T, String)) -> Self {
        Self {
            pending_dirs: vec![root],
            raw_files: Vec::new(),
        }
    }

    fn next_dir(&mut self) -> Option<(T, String)> {
        self.pending_dirs.pop()
    }

    fn extend(&mut self, entries: DirectoryEntries<T>) {
        self.pending_dirs.extend(entries.child_dirs);
        self.raw_files.extend(entries.raw_files);
    }

    fn into_raw_files(self) -> Vec<RawFile> {
        self.raw_files
    }
}

impl<L, S, M> Importer<L, S, M>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
{
    pub async fn import_from_share_url(
        &mut self,
        url: &ShareUrl<'_>,
    ) -> AppResult<Vec<ImportedMedia>> {
        info!("Importing from share URL: {}", url.get_url());
        match url {
            ShareUrl::Pan123(url) => self.import_pan123_share(url).await,
            ShareUrl::Pan189(url) => self.import_pan189_share(url).await,
            ShareUrl::Pan115(url) => self.import_pan115_share(url).await,
        }
    }

    async fn import_pan123_share(&mut self, url: &Url) -> AppResult<Vec<ImportedMedia>> {
        let (share_key, share_password) = parse_pan123_share_parts(url);

        let media_files = self
            .list_files_from_pan123_share(share_key.as_str(), share_password.as_str())
            .await?;
        self.finish_share_import("pan123", media_files).await
    }

    async fn import_pan189_share(&mut self, url: &Url) -> AppResult<Vec<ImportedMedia>> {
        let share_code = parse_pan189_share_code(url);
        if share_code.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share code from URL: {}",
                url
            )));
        }

        let media_files = self.list_files_from_pan189_share(&share_code).await?;
        self.finish_share_import("pan189", media_files).await
    }

    async fn import_pan115_share(&mut self, url: &Url) -> AppResult<Vec<ImportedMedia>> {
        let (share_code, receive_code) = parse_pan115_share_parts(url);

        if share_code.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share code from URL: {}",
                url
            )));
        }

        let media_files = self
            .list_files_from_pan115_share(&share_code, &receive_code)
            .await?;
        self.finish_share_import("pan115", media_files).await
    }

    async fn finish_share_import(
        &mut self,
        provider: &str,
        media_files: Vec<MediaFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        info!(
            "found {} media files from {} share",
            media_files.len(),
            provider
        );
        self.transfer_media_files(&media_files).await
    }

    async fn list_files_from_pan123_share(
        &mut self,
        share_key: &str,
        share_password: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let mut traversal = ShareTraversal::new((0, String::new()));

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let files = self
                .share_remote
                .list_pan123_share_files(share_key, share_password, parent_id)
                .await?;

            traversal.extend(collect_pan123_directory_entries(&files, &parent_path));
        }

        Ok(self.build_media_files(traversal.into_raw_files()))
    }

    async fn list_files_from_pan189_share(
        &mut self,
        share_code: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let share_info = self.share_remote.get_pan189_share_info(share_code).await?;

        let mut traversal =
            ShareTraversal::new((share_info.file_id, share_info.file_name.to_owned()));

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let (folders, files) = self
                .share_remote
                .list_pan189_share_files(share_info.share_id, share_info.share_mode, &parent_id)
                .await?;

            traversal.extend(collect_pan189_directory_entries(
                &folders,
                &files,
                &parent_path,
            ));
        }

        Ok(self.build_media_files(traversal.into_raw_files()))
    }

    async fn list_files_from_pan115_share(
        &mut self,
        share_code: &str,
        receive_code: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let mut traversal = ShareTraversal::new(("0".to_string(), String::new()));

        while let Some((cid, parent_path)) = traversal.next_dir() {
            let entries = self
                .share_remote
                .list_pan115_share_files(share_code, receive_code, &cid)
                .await?;

            traversal.extend(collect_pan115_directory_entries(&entries, &parent_path));
        }

        Ok(self.build_media_files(traversal.into_raw_files()))
    }
}

fn collect_pan123_directory_entries(
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

fn collect_pan189_directory_entries(
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

fn collect_pan115_directory_entries(
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

fn child_share_path(parent_path: &str, name: &str) -> String {
    format!("{}/{}", parent_path, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::import::inner::Etag;

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
