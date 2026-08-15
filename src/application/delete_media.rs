use crate::{
    application::{
        import_local_store::ImportLocalStore,
        ports::{LibraryGateway, LibraryGatewayHandle, MediaDirectoryRecord},
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

#[derive(Clone)]
pub struct DeleteMediaService {
    library: LibraryGatewayHandle,
    local: ImportLocalStore,
    root_path: String,
}

impl DeleteMediaService {
    pub fn new(
        library: impl LibraryGateway + 'static,
        local: ImportLocalStore,
        root_path: String,
    ) -> Self {
        Self {
            library: std::sync::Arc::new(library),
            local,
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
            .filter(|record| record.display_name.contains("tmdb-"))
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

    pub async fn delete_candidate(&self, candidate: &MediaDeleteCandidate) -> AppResult<()> {
        self.library
            .trash_library_files(&[candidate.dir_id])
            .await?;

        let local_path = self
            .local
            .local_path_for_remote(candidate.remote_path.as_str());
        self.local
            .remove_local_dir_if_exists(local_path.as_str())
            .await
    }
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
    use crate::application::ports::{FileStore, LocalEntry, MediaDirectoryRecord};
    use crate::domain::share::FileHash;

    #[derive(Clone, Default)]
    struct FakeLibraryGateway {
        records: Arc<Vec<MediaDirectoryRecord>>,
        trashed: Arc<Mutex<Vec<Vec<i64>>>>,
    }

    #[async_trait::async_trait]
    impl LibraryGateway for FakeLibraryGateway {
        async fn list_library_files(
            &self,
            _dir_id: i64,
        ) -> AppResult<Vec<crate::domain::import::LibraryFile>> {
            unimplemented!()
        }

        async fn get_library_dir_id_by_path(&self, _path: &str) -> AppResult<Option<i64>> {
            unimplemented!()
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
        );

        let candidates = service.search_candidates("bad").await.unwrap();

        assert!(candidates.is_empty());
    }

    #[tokio::test]
    async fn delete_candidate_trashes_remote_dir_and_local_dir() {
        let library = FakeLibraryGateway::default();
        let file_store = RecordingFileStore::default();
        let service = DeleteMediaService::new(
            library.clone(),
            local_store(file_store.clone()),
            "/remote".to_string(),
        );
        let candidate = MediaDeleteCandidate {
            dir_id: 77,
            remote_path: "/remote/电影/欧美/Inception (2010) {tmdb-27205}".to_string(),
            relative_path: "电影/欧美/Inception (2010) {tmdb-27205}".to_string(),
            display_name: "Inception (2010) {tmdb-27205}".to_string(),
        };

        service.delete_candidate(&candidate).await.unwrap();

        assert_eq!(library.trashed.lock().unwrap().as_slice(), &[vec![77]]);
        assert_eq!(
            file_store.removed_dirs.lock().unwrap().as_slice(),
            &[String::from(
                "/local/电影/欧美/Inception (2010) {tmdb-27205}"
            )]
        );
    }
}
