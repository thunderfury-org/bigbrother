use tracing::info;

use crate::{
    domain::{
        library::{
            path_mapping::SyncPathMapper,
            sync_plan::{LocalNode, PlannedFile, PlannedFileKind, SyncPlan, build_sync_plan},
        },
        media::{FileType, Metadata},
    },
    error::{AppError, AppResult},
};

use super::ports::{FileStore, FileStoreHandle, LibraryGateway, LibraryGatewayHandle};

#[derive(Debug, Clone)]
pub struct SyncStrmConfig {
    pub remote_path: String,
    pub local_path: String,
    pub strm_download_url: String,
}

#[derive(Clone)]
pub struct SyncStrmService {
    remote: LibraryGatewayHandle,
    file_store: FileStoreHandle,
    config: SyncStrmConfig,
}

impl SyncStrmService {
    pub fn new(
        remote: impl LibraryGateway + 'static,
        file_store: impl FileStore + 'static,
        config: SyncStrmConfig,
    ) -> Self {
        Self {
            remote: std::sync::Arc::new(remote),
            file_store: std::sync::Arc::new(file_store),
            config,
        }
    }
}

impl SyncStrmService {
    pub async fn execute(&self) -> AppResult<()> {
        let remote_path = self.config.remote_path.clone();
        let root_id = self
            .remote
            .get_library_dir_id_by_path(&remote_path)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("remote path not found: {remote_path}")))?;
        let mapper = SyncPathMapper::new(
            self.config.remote_path.clone(),
            self.config.local_path.clone(),
        );
        let remote_snapshot = self
            .collect_remote_snapshot(&mapper, &remote_path, root_id)
            .await?;
        let local_entries = self
            .collect_local_nodes(self.config.local_path.as_str())
            .await?;
        let plan = build_sync_plan(
            remote_snapshot.files,
            remote_snapshot.directories,
            local_entries,
        );

        self.execute_plan(plan).await
    }

    async fn collect_remote_snapshot(
        &self,
        mapper: &SyncPathMapper,
        path: &str,
        dir_id: i64,
    ) -> AppResult<RemoteSnapshot> {
        let mut stack = vec![(path.to_string(), dir_id)];
        let mut planned_files = Vec::new();
        let mut directories = Vec::new();

        while let Some((current_path, current_dir_id)) = stack.pop() {
            directories.push(mapper.remote_to_local_path(current_path.as_str()));
            let files = self.remote.list_library_files(current_dir_id).await?;
            for file in files {
                let file_path = format!("{current_path}/{}", file.file_name);
                let meta = Metadata::parse(&file.file_name);
                if file.is_dir {
                    stack.push((file_path, file.file_id));
                } else if meta.is_video() {
                    planned_files.push(PlannedFile {
                        local_path: mapper.remote_to_local_strm_path(&file_path, &meta.extension),
                        kind: PlannedFileKind::Strm {
                            remote_path: file_path,
                            file_id: file.file_id,
                        },
                    });
                } else if meta.file_type == FileType::Subtitle {
                    planned_files.push(PlannedFile {
                        local_path: mapper.remote_to_local_path(&file_path),
                        kind: PlannedFileKind::Subtitle {
                            file_id: file.file_id,
                            remote_size: file.size,
                        },
                    });
                }
            }
        }

        Ok(RemoteSnapshot {
            files: planned_files,
            directories,
        })
    }

    async fn collect_local_nodes(&self, root_dir: &str) -> AppResult<Vec<LocalNode>> {
        let mut stack = vec![root_dir.to_string()];
        let mut nodes = Vec::new();

        while let Some(current_dir) = stack.pop() {
            for entry in self.file_store.read_dir(current_dir.as_str()).await? {
                if entry.is_dir {
                    stack.push(entry.path.clone());
                }
                nodes.push(LocalNode {
                    path: entry.path,
                    is_dir: entry.is_dir,
                });
            }
        }

        Ok(nodes)
    }

    async fn execute_plan(&self, plan: SyncPlan) -> AppResult<()> {
        for file in plan.files {
            match file.kind {
                PlannedFileKind::Strm {
                    remote_path,
                    file_id,
                } => {
                    self.sync_strm_file(&remote_path, file_id, file.local_path.as_str())
                        .await?;
                }
                PlannedFileKind::Subtitle {
                    file_id,
                    remote_size,
                } => {
                    self.sync_subtitle_file(file_id, remote_size, file.local_path.as_str())
                        .await?;
                }
            }
        }

        for stale_file in plan.stale_files {
            self.file_store.remove_file(stale_file.as_str()).await?;
            info!("Deleted stale local file: {stale_file}");
        }

        for stale_dir in plan.stale_dirs {
            self.file_store.remove_dir_all(stale_dir.as_str()).await?;
            info!("Deleted stale local directory: {stale_dir}");
        }

        Ok(())
    }

    async fn sync_strm_file(
        &self,
        remote_file_path: &str,
        file_id: i64,
        local_path: &str,
    ) -> AppResult<()> {
        let expected_url = format!(
            "{}{}?file_id={}",
            self.config.strm_download_url, remote_file_path, file_id
        );

        if let Some(existing) = self.file_store.read_to_string_if_exists(local_path).await? {
            if existing == expected_url {
                return Ok(());
            }
            self.file_store
                .write(local_path, expected_url.as_bytes())
                .await?;
            info!("Strm file updated: {local_path}");
            return Ok(());
        }

        self.file_store.ensure_parent_dir(local_path).await?;
        self.file_store
            .write(local_path, expected_url.as_bytes())
            .await?;
        info!("Strm file created: {local_path}");
        Ok(())
    }

    async fn sync_subtitle_file(
        &self,
        file_id: i64,
        remote_size: u64,
        local_path: &str,
    ) -> AppResult<()> {
        if let Some(size) = self.file_store.metadata_len_if_exists(local_path).await? {
            if size == remote_size {
                return Ok(());
            }
            self.remote
                .download_library_file(file_id, local_path)
                .await?;
            info!("Subtitle file updated: {local_path}");
            return Ok(());
        }

        self.file_store.ensure_parent_dir(local_path).await?;
        self.remote
            .download_library_file(file_id, local_path)
            .await?;
        info!("Subtitle file created: {local_path}");
        Ok(())
    }
}

struct RemoteSnapshot {
    files: Vec<PlannedFile>,
    directories: Vec<String>,
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use crate::application::ports::{LocalEntry, MediaDirectoryRecord};
    use crate::domain::import::LibraryFile;
    use crate::domain::share::FileHash;

    use super::*;

    #[derive(Clone, Default)]
    struct FakeRemote {
        root_ids: Arc<Mutex<HashMap<String, i64>>>,
        dirs: Arc<Mutex<HashMap<i64, Vec<LibraryFile>>>>,
        downloads: Arc<Mutex<Vec<(i64, String)>>>,
        fail_download: Arc<Mutex<bool>>,
    }

    #[async_trait::async_trait]
    impl LibraryGateway for FakeRemote {
        async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
            Ok(self.root_ids.lock().unwrap().get(path).copied())
        }

        async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>> {
            Ok(self
                .dirs
                .lock()
                .unwrap()
                .get(&dir_id)
                .cloned()
                .unwrap_or_default())
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

        async fn trash_library_files(&self, _file_ids: &[i64]) -> AppResult<()> {
            unimplemented!()
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

        async fn download_library_file(&self, file_id: i64, local_path: &str) -> AppResult<()> {
            if *self.fail_download.lock().unwrap() {
                return Err(AppError::ExternalService(
                    "download failed".to_string(),
                    false,
                ));
            }
            self.downloads
                .lock()
                .unwrap()
                .push((file_id, local_path.to_string()));
            Ok(())
        }

        async fn search_media_dirs(&self, _keyword: &str) -> AppResult<Vec<MediaDirectoryRecord>> {
            unimplemented!()
        }
    }

    #[derive(Clone, Default)]
    struct FakeFileStore {
        strings: Arc<Mutex<HashMap<String, String>>>,
        sizes: Arc<Mutex<HashMap<String, u64>>>,
        dirs: Arc<Mutex<HashMap<String, Vec<LocalEntry>>>>,
        ensured: Arc<Mutex<Vec<String>>>,
        writes: Arc<Mutex<Vec<(String, String)>>>,
        removed_files: Arc<Mutex<Vec<String>>>,
        removed_dirs: Arc<Mutex<Vec<String>>>,
        fail_write: Arc<Mutex<bool>>,
        fail_remove_dir: Arc<Mutex<bool>>,
    }

    #[async_trait::async_trait]
    impl FileStore for FakeFileStore {
        async fn read_to_string_if_exists(&self, path: &str) -> AppResult<Option<String>> {
            Ok(self.strings.lock().unwrap().get(path).cloned())
        }

        async fn metadata_len_if_exists(&self, path: &str) -> AppResult<Option<u64>> {
            Ok(self.sizes.lock().unwrap().get(path).copied())
        }

        async fn ensure_parent_dir(&self, path: &str) -> AppResult<()> {
            self.ensured.lock().unwrap().push(path.to_string());
            Ok(())
        }

        async fn write(&self, path: &str, content: &[u8]) -> AppResult<()> {
            if *self.fail_write.lock().unwrap() {
                return Err(AppError::Internal("write failed".to_string()));
            }
            let text = String::from_utf8(content.to_vec()).unwrap();
            self.writes
                .lock()
                .unwrap()
                .push((path.to_string(), text.clone()));
            self.strings.lock().unwrap().insert(path.to_string(), text);
            Ok(())
        }

        async fn read_dir(&self, path: &str) -> AppResult<Vec<LocalEntry>> {
            Ok(self
                .dirs
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .unwrap_or_default())
        }

        async fn remove_file(&self, path: &str) -> AppResult<()> {
            self.removed_files.lock().unwrap().push(path.to_string());
            Ok(())
        }

        async fn remove_dir_all(&self, path: &str) -> AppResult<()> {
            if *self.fail_remove_dir.lock().unwrap() {
                return Err(AppError::Internal("remove dir failed".to_string()));
            }
            self.removed_dirs.lock().unwrap().push(path.to_string());
            Ok(())
        }

        async fn remove_file_if_exists(&self, path: &str) -> AppResult<()> {
            self.remove_file(path).await
        }

        async fn remove_dir_all_if_exists(&self, path: &str) -> AppResult<()> {
            self.remove_dir_all(path).await
        }
    }

    #[tokio::test]
    async fn execute_syncs_expected_files_and_removes_stale_entries() {
        let remote = FakeRemote::default();
        remote
            .root_ids
            .lock()
            .unwrap()
            .insert("/remote".to_string(), 1);
        remote.dirs.lock().unwrap().insert(
            1,
            vec![
                LibraryFile {
                    file_id: 2,
                    file_name: "show".to_string(),
                    is_dir: true,
                    size: 0,
                    hash: String::new(),
                },
                LibraryFile {
                    file_id: 3,
                    file_name: "Movie.2024.1080p.WEB-DL.mkv".to_string(),
                    is_dir: false,
                    size: 100,
                    hash: String::new(),
                },
            ],
        );
        remote.dirs.lock().unwrap().insert(
            2,
            vec![LibraryFile {
                file_id: 4,
                file_name: "《LV1魔王與獨居廢勇者》#7 (簡中字幕)【Ani-One Asia】.zh-Hans.srt"
                    .to_string(),
                is_dir: false,
                size: 8,
                hash: String::new(),
            }],
        );

        let file_store = FakeFileStore::default();
        file_store.dirs.lock().unwrap().insert(
            "/local".to_string(),
            vec![
                LocalEntry {
                    path: "/local/show".to_string(),
                    is_dir: true,
                },
                LocalEntry {
                    path: "/local/stale.strm".to_string(),
                    is_dir: false,
                },
                LocalEntry {
                    path: "/local/obsolete".to_string(),
                    is_dir: true,
                },
            ],
        );
        file_store.dirs.lock().unwrap().insert(
            "/local/show".to_string(),
            vec![LocalEntry {
                path: "/local/show/old.srt".to_string(),
                is_dir: false,
            }],
        );
        file_store.dirs.lock().unwrap().insert(
            "/local/obsolete".to_string(),
            vec![LocalEntry {
                path: "/local/obsolete/nested".to_string(),
                is_dir: true,
            }],
        );

        let service = SyncStrmService::new(
            remote.clone(),
            file_store.clone(),
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
        );

        service.execute().await.unwrap();

        assert_eq!(
            file_store.removed_files.lock().unwrap().as_slice(),
            ["/local/show/old.srt", "/local/stale.strm"]
        );
        assert_eq!(
            file_store.removed_dirs.lock().unwrap().as_slice(),
            ["/local/obsolete"]
        );
        assert_eq!(
            remote.downloads.lock().unwrap().as_slice(),
            [(
                4,
                "/local/show/《LV1魔王與獨居廢勇者》#7 (簡中字幕)【Ani-One Asia】.zh-Hans.srt"
                    .to_string(),
            )]
        );
        assert!(
            file_store
                .writes
                .lock()
                .unwrap()
                .iter()
                .any(
                    |(path, content)| path == "/local/Movie.2024.1080p.WEB-DL.strm"
                        && content == "https://host/d/remote/Movie.2024.1080p.WEB-DL.mkv?file_id=3"
                )
        );
    }

    #[tokio::test]
    async fn execute_returns_not_found_when_remote_root_is_missing() {
        let service = SyncStrmService::new(
            FakeRemote::default(),
            FakeFileStore::default(),
            SyncStrmConfig {
                remote_path: "/missing".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
        );

        let error = service.execute().await.unwrap_err();

        assert!(matches!(error, AppError::NotFound(message) if message.contains("/missing")));
    }

    #[tokio::test]
    async fn execute_skips_when_existing_strm_and_subtitle_are_current() {
        let remote = FakeRemote::default();
        remote
            .root_ids
            .lock()
            .unwrap()
            .insert("/remote".to_string(), 1);
        remote.dirs.lock().unwrap().insert(
            1,
            vec![
                LibraryFile {
                    file_id: 3,
                    file_name: "Movie.2024.1080p.WEB-DL.mkv".to_string(),
                    is_dir: false,
                    size: 100,
                    hash: String::new(),
                },
                LibraryFile {
                    file_id: 4,
                    file_name: "Movie.2024.1080p.WEB-DL.zh.srt".to_string(),
                    is_dir: false,
                    size: 8,
                    hash: String::new(),
                },
            ],
        );

        let file_store = FakeFileStore::default();
        file_store.strings.lock().unwrap().insert(
            "/local/Movie.2024.1080p.WEB-DL.strm".to_string(),
            "https://host/d/remote/Movie.2024.1080p.WEB-DL.mkv?file_id=3".to_string(),
        );
        file_store
            .sizes
            .lock()
            .unwrap()
            .insert("/local/Movie.2024.1080p.WEB-DL.zh.srt".to_string(), 8);

        let service = SyncStrmService::new(
            remote.clone(),
            file_store.clone(),
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
        );

        service.execute().await.unwrap();

        assert!(file_store.writes.lock().unwrap().is_empty());
        assert!(file_store.ensured.lock().unwrap().is_empty());
        assert!(remote.downloads.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn execute_propagates_download_failure() {
        let remote = FakeRemote::default();
        remote
            .root_ids
            .lock()
            .unwrap()
            .insert("/remote".to_string(), 1);
        remote.dirs.lock().unwrap().insert(
            1,
            vec![LibraryFile {
                file_id: 4,
                file_name: "Movie.2024.1080p.WEB-DL.zh.srt".to_string(),
                is_dir: false,
                size: 8,
                hash: String::new(),
            }],
        );
        *remote.fail_download.lock().unwrap() = true;

        let service = SyncStrmService::new(
            remote,
            FakeFileStore::default(),
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
        );

        let error = service.execute().await.unwrap_err();

        assert!(
            matches!(error, AppError::ExternalService(message, _) if message.contains("download failed"))
        );
    }

    #[tokio::test]
    async fn execute_propagates_write_failure() {
        let remote = FakeRemote::default();
        remote
            .root_ids
            .lock()
            .unwrap()
            .insert("/remote".to_string(), 1);
        remote.dirs.lock().unwrap().insert(
            1,
            vec![LibraryFile {
                file_id: 3,
                file_name: "Movie.2024.1080p.WEB-DL.mkv".to_string(),
                is_dir: false,
                size: 100,
                hash: String::new(),
            }],
        );

        let file_store = FakeFileStore::default();
        *file_store.fail_write.lock().unwrap() = true;

        let service = SyncStrmService::new(
            remote,
            file_store,
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
        );

        let error = service.execute().await.unwrap_err();

        assert!(matches!(error, AppError::Internal(message) if message.contains("write failed")));
    }

    #[tokio::test]
    async fn execute_propagates_stale_directory_removal_failure() {
        let remote = FakeRemote::default();
        remote
            .root_ids
            .lock()
            .unwrap()
            .insert("/remote".to_string(), 1);
        remote.dirs.lock().unwrap().insert(1, Vec::new());

        let file_store = FakeFileStore::default();
        file_store.dirs.lock().unwrap().insert(
            "/local".to_string(),
            vec![LocalEntry {
                path: "/local/obsolete".to_string(),
                is_dir: true,
            }],
        );
        *file_store.fail_remove_dir.lock().unwrap() = true;

        let service = SyncStrmService::new(
            remote,
            file_store,
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
        );

        let error = service.execute().await.unwrap_err();

        assert!(
            matches!(error, AppError::Internal(message) if message.contains("remove dir failed"))
        );
    }
}
