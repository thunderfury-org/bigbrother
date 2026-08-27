use crate::{
    application::{
        import_local_store::ImportLocalStore,
        ports::{
            LibraryGateway, LibraryGatewayHandle, LibraryMediaUpdate, LibraryMediaUpdateKind,
            LibraryUpdateNotifierHandle, MediaDirectoryRecord, notify_library_updates,
        },
    },
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDeleteCandidate {
    pub dir_id: i64,
    pub remote_path: String,
    pub relative_path: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDirEntry {
    pub dir_id: i64,
    pub display_name: String,
    pub deletable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDirDeleteItem {
    pub dir_id: i64,
    pub relative_path: String,
}

#[derive(Clone)]
pub struct DeleteMediaService {
    library: LibraryGatewayHandle,
    local: ImportLocalStore,
    notifier: LibraryUpdateNotifierHandle,
    root_path: String,
}

impl DeleteMediaService {
    pub fn new(
        library: impl LibraryGateway + 'static,
        local: ImportLocalStore,
        root_path: String,
        notifier: LibraryUpdateNotifierHandle,
    ) -> Self {
        Self {
            library: std::sync::Arc::new(library),
            local,
            notifier,
            root_path,
        }
    }
}

impl DeleteMediaService {
    pub async fn search_candidates(&self, keyword: &str) -> AppResult<Vec<MediaDeleteCandidate>> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err(AppError::InvalidParameter("keyword is empty".to_owned()));
        }

        let root = normalize_root_path(self.root_path.as_str());
        let mut candidates = self
            .library
            .search_media_dirs(keyword)
            .await?
            .into_iter()
            .filter(|record| is_deletable_media_name(&record.display_name))
            .filter_map(|record| to_candidate(record, root.as_str()))
            .collect::<Vec<_>>();

        candidates.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });
        candidates.truncate(10);

        Ok(candidates)
    }

    pub async fn list_children(&self, parent_id: Option<i64>) -> AppResult<Vec<MediaDirEntry>> {
        let dir_id = match parent_id {
            Some(id) => id,
            None => {
                let root = normalize_root_path(self.root_path.as_str());
                self.library
                    .get_library_dir_id_by_path(root.as_str())
                    .await?
                    .ok_or_else(|| AppError::NotFound(format!("library root not found: {root}")))?
            }
        };

        let mut entries = self
            .library
            .list_library_files(dir_id)
            .await?
            .into_iter()
            .filter(|file| file.is_dir)
            .map(|file| MediaDirEntry {
                dir_id: file.file_id,
                display_name: file.file_name.clone(),
                deletable: is_deletable_media_name(&file.file_name),
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        Ok(entries)
    }

    pub async fn delete_dirs(&self, items: &[MediaDirDeleteItem]) -> AppResult<()> {
        if items.is_empty() {
            return Err(AppError::InvalidParameter("items is empty".to_owned()));
        }

        let root = normalize_root_path(self.root_path.as_str());
        let mut candidates = Vec::with_capacity(items.len());
        for item in items {
            candidates.push(candidate_from_relative_path(
                item.dir_id,
                item.relative_path.as_str(),
                root.as_str(),
            )?);
        }

        let dir_ids = candidates
            .iter()
            .map(|candidate| candidate.dir_id)
            .collect::<Vec<_>>();
        self.library.trash_library_files(&dir_ids).await?;

        let mut updates = Vec::new();
        let mut result = Ok(());
        for candidate in &candidates {
            let local_path = self
                .local
                .local_path_for_remote(candidate.remote_path.as_str());
            match self
                .local
                .remove_local_dir_if_exists(local_path.as_str())
                .await
            {
                Ok(()) => updates.push(LibraryMediaUpdate {
                    path: local_path,
                    kind: LibraryMediaUpdateKind::Deleted,
                }),
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        notify_library_updates(self.notifier.as_ref(), &updates).await;
        result
    }
}

fn is_deletable_media_name(name: &str) -> bool {
    name.contains("tmdb-")
}

fn candidate_from_relative_path(
    dir_id: i64,
    relative_path: &str,
    root_path: &str,
) -> AppResult<MediaDeleteCandidate> {
    let relative_path = normalize_relative_path(relative_path)?;
    let display_name = relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path.as_str())
        .to_owned();
    if !is_deletable_media_name(&display_name) {
        return Err(AppError::InvalidParameter(format!(
            "not a media directory: {relative_path}"
        )));
    }

    let remote_path = if root_path == "/" {
        format!("/{relative_path}")
    } else {
        format!("{root_path}/{relative_path}")
    };

    Ok(MediaDeleteCandidate {
        dir_id,
        remote_path,
        relative_path,
        display_name,
    })
}

fn normalize_relative_path(path: &str) -> AppResult<String> {
    let trimmed = path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::InvalidParameter(
            "relative_path is empty".to_owned(),
        ));
    }

    let mut parts = Vec::new();
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(AppError::InvalidParameter(
                "relative_path must not contain '.' or '..'".to_owned(),
            ));
        }
        parts.push(part);
    }

    Ok(parts.join("/"))
}

fn to_candidate(record: MediaDirectoryRecord, root_path: &str) -> Option<MediaDeleteCandidate> {
    let normalized_remote_path = normalize_root_path(record.remote_path.as_str());
    let relative_path = strip_root_prefix(normalized_remote_path.as_str(), root_path)?.to_owned();

    Some(MediaDeleteCandidate {
        dir_id: record.dir_id,
        remote_path: normalized_remote_path,
        relative_path,
        display_name: record.display_name,
    })
}

fn strip_root_prefix<'a>(path: &'a str, root_path: &str) -> Option<&'a str> {
    if root_path == "/" {
        let relative = path.trim_start_matches('/');
        return (!relative.is_empty()).then_some(relative);
    }

    let remainder = path.strip_prefix(root_path)?;
    let relative = remainder.strip_prefix('/')?;
    (!relative.is_empty()).then_some(relative)
}

fn normalize_root_path(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::application::ports::{
        FileStore, LocalEntry, MediaDirectoryRecord, NoopLibraryUpdateNotifier,
        library_update::test_support::RecordingLibraryUpdateNotifier,
    };
    use crate::domain::import::LibraryFile;
    use crate::domain::share::FileHash;

    #[derive(Clone, Default)]
    struct FakeLibraryGateway {
        records: Arc<Vec<MediaDirectoryRecord>>,
        trashed: Arc<Mutex<Vec<Vec<i64>>>>,
        root_path: Option<String>,
        root_id: Option<i64>,
        children: Arc<std::collections::HashMap<i64, Vec<LibraryFile>>>,
        path_lookups: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl LibraryGateway for FakeLibraryGateway {
        async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>> {
            Ok(self.children.get(&dir_id).cloned().unwrap_or_default())
        }

        async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
            self.path_lookups.lock().unwrap().push(path.to_owned());
            if self.root_path.as_deref() == Some(path) {
                return Ok(self.root_id);
            }
            Ok(None)
        }

        async fn ensure_dir(&self, _path: &str) -> AppResult<i64> {
            unimplemented!()
        }

        async fn list_library_dir_ids(
            &self,
            _dir_id: i64,
        ) -> AppResult<std::collections::HashMap<String, i64>> {
            unimplemented!()
        }

        async fn mkdir_library_dir(
            &self,
            _parent_dir_id: i64,
            _folder_name: &str,
        ) -> AppResult<i64> {
            unimplemented!()
        }

        async fn trash_library_files(&self, file_ids: &[i64]) -> AppResult<()> {
            self.trashed.lock().unwrap().push(file_ids.to_vec());
            Ok(())
        }

        async fn upload(
            &self,
            _parent_dir_id: i64,
            _file_name: &str,
            _hash: &FileHash,
            _size: u64,
        ) -> AppResult<Option<i64>> {
            unimplemented!()
        }

        async fn download_library_file(&self, _file_id: i64, _local_path: &str) -> AppResult<()> {
            unimplemented!()
        }

        async fn search_media_dirs(&self, _keyword: &str) -> AppResult<Vec<MediaDirectoryRecord>> {
            Ok(self.records.as_ref().clone())
        }
    }

    #[derive(Clone, Default)]
    struct RecordingFileStore {
        removed_dirs: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl FileStore for RecordingFileStore {
        async fn read_to_string_if_exists(&self, _path: &str) -> AppResult<Option<String>> {
            Ok(None)
        }
        async fn metadata_len_if_exists(&self, _path: &str) -> AppResult<Option<u64>> {
            Ok(None)
        }
        async fn ensure_parent_dir(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
        async fn write(&self, _path: &str, _content: &[u8]) -> AppResult<()> {
            Ok(())
        }
        async fn read_dir(&self, _path: &str) -> AppResult<Vec<LocalEntry>> {
            Ok(Vec::new())
        }
        async fn remove_file(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
        async fn remove_dir_all(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
        async fn remove_file_if_exists(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
        async fn remove_dir_all_if_exists(&self, path: &str) -> AppResult<()> {
            self.removed_dirs.lock().unwrap().push(path.to_owned());
            Ok(())
        }
    }

    fn local_store(file_store: RecordingFileStore) -> ImportLocalStore {
        ImportLocalStore::new(
            file_store,
            "/remote".into(),
            "/local".into(),
            "http://d".into(),
        )
    }

    #[tokio::test]
    async fn search_candidates_keeps_tmdb_dirs_under_root() {
        let service = DeleteMediaService::new(
            FakeLibraryGateway {
                records: Arc::new(vec![
                    MediaDirectoryRecord {
                        dir_id: 1,
                        display_name: "Breaking Bad (2008) {tmdb-1396}".to_string(),
                        remote_path: "/remote/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}"
                            .to_string(),
                    },
                    MediaDirectoryRecord {
                        dir_id: 2,
                        display_name: "not-media".to_string(),
                        remote_path: "/remote/misc/not-media".to_string(),
                    },
                    MediaDirectoryRecord {
                        dir_id: 3,
                        display_name: "Alien (1979) {tmdb-348}".to_string(),
                        remote_path: "/other/电影/欧美/Alien (1979) {tmdb-348}".to_string(),
                    },
                ]),
                ..FakeLibraryGateway::default()
            },
            local_store(RecordingFileStore::default()),
            "/remote".to_string(),
            std::sync::Arc::new(NoopLibraryUpdateNotifier),
        );

        let candidates = service.search_candidates("bad").await.unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].dir_id, 1);
        assert_eq!(
            candidates[0].relative_path,
            "电视剧/欧美/Breaking Bad (2008) {tmdb-1396}"
        );
    }

    #[tokio::test]
    async fn search_candidates_rejects_paths_outside_root_with_same_text_prefix() {
        let service = DeleteMediaService::new(
            FakeLibraryGateway {
                records: Arc::new(vec![MediaDirectoryRecord {
                    dir_id: 1,
                    display_name: "Breaking Bad (2008) {tmdb-1396}".to_string(),
                    remote_path: "/remote_backup/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}"
                        .to_string(),
                }]),
                ..FakeLibraryGateway::default()
            },
            local_store(RecordingFileStore::default()),
            "/remote".to_string(),
            std::sync::Arc::new(NoopLibraryUpdateNotifier),
        );

        let candidates = service.search_candidates("bad").await.unwrap();

        assert!(candidates.is_empty());
    }

    fn dir_file(file_id: i64, file_name: &str) -> LibraryFile {
        LibraryFile {
            file_id,
            file_name: file_name.to_owned(),
            is_dir: true,
            size: 0,
            hash: String::new(),
        }
    }

    fn file_entry(file_id: i64, file_name: &str) -> LibraryFile {
        LibraryFile {
            file_id,
            file_name: file_name.to_owned(),
            is_dir: false,
            size: 10,
            hash: String::new(),
        }
    }

    fn browse_gateway() -> FakeLibraryGateway {
        let mut children = std::collections::HashMap::new();
        children.insert(
            1,
            vec![
                file_entry(11, "readme.txt"),
                dir_file(3, "电视剧"),
                dir_file(2, "电影"),
            ],
        );
        children.insert(2, vec![dir_file(21, "欧美"), dir_file(22, "国产")]);
        children.insert(
            21,
            vec![
                dir_file(211, "Inception (2010) {tmdb-27205}"),
                file_entry(212, "notes.txt"),
                dir_file(213, "misc"),
            ],
        );
        FakeLibraryGateway {
            root_path: Some("/remote".into()),
            root_id: Some(1),
            children: Arc::new(children),
            ..FakeLibraryGateway::default()
        }
    }

    #[tokio::test]
    async fn list_children_resolves_root_path_only_when_parent_id_is_absent() {
        let library = browse_gateway();
        let service = DeleteMediaService::new(
            library.clone(),
            local_store(RecordingFileStore::default()),
            "/remote".to_string(),
            std::sync::Arc::new(NoopLibraryUpdateNotifier),
        );

        let root_entries = service.list_children(None).await.unwrap();
        assert_eq!(
            library.path_lookups.lock().unwrap().as_slice(),
            &["/remote".to_string()]
        );
        assert_eq!(
            root_entries,
            vec![
                MediaDirEntry {
                    dir_id: 2,
                    display_name: "电影".into(),
                    deletable: false,
                },
                MediaDirEntry {
                    dir_id: 3,
                    display_name: "电视剧".into(),
                    deletable: false,
                },
            ]
        );

        let child_entries = service.list_children(Some(21)).await.unwrap();
        assert_eq!(library.path_lookups.lock().unwrap().len(), 1);
        assert_eq!(
            child_entries,
            vec![
                MediaDirEntry {
                    dir_id: 211,
                    display_name: "Inception (2010) {tmdb-27205}".into(),
                    deletable: true,
                },
                MediaDirEntry {
                    dir_id: 213,
                    display_name: "misc".into(),
                    deletable: false,
                },
            ]
        );
    }

    #[tokio::test]
    async fn list_children_returns_not_found_when_root_is_missing() {
        let service = DeleteMediaService::new(
            FakeLibraryGateway::default(),
            local_store(RecordingFileStore::default()),
            "/remote".to_string(),
            std::sync::Arc::new(NoopLibraryUpdateNotifier),
        );

        let err = service.list_children(None).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_dirs_trashes_all_ids_once_and_removes_local_dirs() {
        let library = FakeLibraryGateway::default();
        let file_store = RecordingFileStore::default();
        let notifier = RecordingLibraryUpdateNotifier::default();
        let service = DeleteMediaService::new(
            library.clone(),
            local_store(file_store.clone()),
            "/remote".to_string(),
            std::sync::Arc::new(notifier.clone()),
        );

        service
            .delete_dirs(&[
                MediaDirDeleteItem {
                    dir_id: 77,
                    relative_path: "电影/欧美/Inception (2010) {tmdb-27205}".into(),
                },
                MediaDirDeleteItem {
                    dir_id: 88,
                    relative_path: "电视剧/欧美/Breaking Bad (2008) {tmdb-1396}".into(),
                },
            ])
            .await
            .unwrap();

        assert_eq!(library.trashed.lock().unwrap().as_slice(), &[vec![77, 88]]);
        assert_eq!(
            file_store.removed_dirs.lock().unwrap().as_slice(),
            &[
                String::from("/local/电影/欧美/Inception (2010) {tmdb-27205}"),
                String::from("/local/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}"),
            ]
        );
        assert_eq!(
            notifier.batches(),
            vec![vec![
                LibraryMediaUpdate {
                    path: "/local/电影/欧美/Inception (2010) {tmdb-27205}".to_string(),
                    kind: LibraryMediaUpdateKind::Deleted,
                },
                LibraryMediaUpdate {
                    path: "/local/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}".to_string(),
                    kind: LibraryMediaUpdateKind::Deleted,
                },
            ]]
        );
    }

    #[tokio::test]
    async fn delete_dirs_succeeds_when_library_notify_fails() {
        let notifier = RecordingLibraryUpdateNotifier::failing();
        let service = DeleteMediaService::new(
            FakeLibraryGateway::default(),
            local_store(RecordingFileStore::default()),
            "/remote".to_string(),
            std::sync::Arc::new(notifier.clone()),
        );

        service
            .delete_dirs(&[MediaDirDeleteItem {
                dir_id: 77,
                relative_path: "电影/欧美/Inception (2010) {tmdb-27205}".into(),
            }])
            .await
            .unwrap();
        assert_eq!(notifier.flat_updates().len(), 1);
    }

    #[tokio::test]
    async fn delete_dirs_rejects_empty_items_parent_paths_and_non_media_names() {
        let library = FakeLibraryGateway::default();
        let service = DeleteMediaService::new(
            library.clone(),
            local_store(RecordingFileStore::default()),
            "/remote".to_string(),
            std::sync::Arc::new(NoopLibraryUpdateNotifier),
        );

        let empty = service.delete_dirs(&[]).await.unwrap_err();
        assert!(matches!(empty, AppError::InvalidParameter(_)));

        let parent = service
            .delete_dirs(&[MediaDirDeleteItem {
                dir_id: 2,
                relative_path: "电影/../Inception (2010) {tmdb-27205}".into(),
            }])
            .await
            .unwrap_err();
        assert!(matches!(parent, AppError::InvalidParameter(_)));

        let category = service
            .delete_dirs(&[MediaDirDeleteItem {
                dir_id: 2,
                relative_path: "电影/欧美".into(),
            }])
            .await
            .unwrap_err();
        assert!(matches!(category, AppError::InvalidParameter(_)));

        assert!(library.trashed.lock().unwrap().is_empty());
    }
}
