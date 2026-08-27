use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tracing::{error, info};

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

use super::ports::{
    FileStore, FileStoreHandle, LibraryGateway, LibraryGatewayHandle, LibraryMediaUpdate,
    LibraryMediaUpdateKind, LibraryUpdateNotifierHandle, notify_library_updates,
};

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
    notifier: LibraryUpdateNotifierHandle,
    config: SyncStrmConfig,
}

impl SyncStrmService {
    pub fn new(
        remote: impl LibraryGateway + 'static,
        file_store: impl FileStore + 'static,
        config: SyncStrmConfig,
        notifier: LibraryUpdateNotifierHandle,
    ) -> Self {
        Self {
            remote: Arc::new(remote),
            file_store: Arc::new(file_store),
            notifier,
            config,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub created: u32,
    pub modified: u32,
    pub deleted: u32,
    pub unchanged: u32,
}

impl SyncReport {
    fn record_change(&mut self, kind: Option<LibraryMediaUpdateKind>) {
        match kind {
            Some(LibraryMediaUpdateKind::Created) => self.created += 1,
            Some(LibraryMediaUpdateKind::Modified) => self.modified += 1,
            Some(LibraryMediaUpdateKind::Deleted) => self.deleted += 1,
            None => self.unchanged += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibrarySyncState {
    Idle,
    Running {
        started_at: DateTime<Utc>,
    },
    Succeeded {
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        report: SyncReport,
    },
    Failed {
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        message: String,
    },
}

#[derive(Clone)]
pub struct LibrarySyncController {
    service: Arc<SyncStrmService>,
    state: Arc<Mutex<LibrarySyncState>>,
}

impl LibrarySyncController {
    pub fn new(service: SyncStrmService) -> Self {
        Self {
            service: Arc::new(service),
            state: Arc::new(Mutex::new(LibrarySyncState::Idle)),
        }
    }

    pub fn snapshot(&self) -> LibrarySyncState {
        self.state
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    pub fn try_start(&self) -> bool {
        let started_at = Utc::now();
        {
            let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
            if matches!(*state, LibrarySyncState::Running { .. }) {
                return false;
            }
            *state = LibrarySyncState::Running { started_at };
        }

        let service = self.service.clone();
        let state = self.state.clone();
        tokio::spawn(async move {
            info!("Starting library strm sync");
            let result = service.execute().await;
            let finished_at = Utc::now();
            let mut guard = state.lock().unwrap_or_else(|err| err.into_inner());
            *guard = match result {
                Ok(report) => {
                    info!(
                        created = report.created,
                        modified = report.modified,
                        deleted = report.deleted,
                        unchanged = report.unchanged,
                        "Library strm sync completed"
                    );
                    LibrarySyncState::Succeeded {
                        started_at,
                        finished_at,
                        report,
                    }
                }
                Err(err) => {
                    error!(error = %err, "Library strm sync failed");
                    LibrarySyncState::Failed {
                        started_at,
                        finished_at,
                        message: err.to_string(),
                    }
                }
            };
        });
        true
    }
}

impl SyncStrmService {
    pub async fn execute(&self) -> AppResult<SyncReport> {
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

    async fn execute_plan(&self, plan: SyncPlan) -> AppResult<SyncReport> {
        let mut updates = Vec::new();
        let mut report = SyncReport::default();
        let result = self.apply_plan(plan, &mut updates, &mut report).await;
        notify_library_updates(self.notifier.as_ref(), &updates).await;
        result?;
        Ok(report)
    }

    async fn apply_plan(
        &self,
        plan: SyncPlan,
        updates: &mut Vec<LibraryMediaUpdate>,
        report: &mut SyncReport,
    ) -> AppResult<()> {
        for file in plan.files {
            let kind = match file.kind {
                PlannedFileKind::Strm {
                    remote_path,
                    file_id,
                } => {
                    self.sync_strm_file(&remote_path, file_id, file.local_path.as_str())
                        .await?
                }
                PlannedFileKind::Subtitle {
                    file_id,
                    remote_size,
                } => {
                    self.sync_subtitle_file(file_id, remote_size, file.local_path.as_str())
                        .await?
                }
            };
            report.record_change(kind);
            if let Some(kind) = kind {
                updates.push(LibraryMediaUpdate {
                    path: file.local_path,
                    kind,
                });
            }
        }

        for stale_file in plan.stale_files {
            self.file_store.remove_file(stale_file.as_str()).await?;
            info!("Deleted stale local file: {stale_file}");
            report.deleted += 1;
            updates.push(LibraryMediaUpdate {
                path: stale_file,
                kind: LibraryMediaUpdateKind::Deleted,
            });
        }

        for stale_dir in plan.stale_dirs {
            self.file_store.remove_dir_all(stale_dir.as_str()).await?;
            info!("Deleted stale local directory: {stale_dir}");
            report.deleted += 1;
            updates.push(LibraryMediaUpdate {
                path: stale_dir,
                kind: LibraryMediaUpdateKind::Deleted,
            });
        }

        Ok(())
    }

    async fn sync_strm_file(
        &self,
        remote_file_path: &str,
        file_id: i64,
        local_path: &str,
    ) -> AppResult<Option<LibraryMediaUpdateKind>> {
        let expected_url = format!(
            "{}{}?file_id={}",
            self.config.strm_download_url, remote_file_path, file_id
        );

        if let Some(existing) = self.file_store.read_to_string_if_exists(local_path).await? {
            if existing == expected_url {
                return Ok(None);
            }
            self.file_store
                .write(local_path, expected_url.as_bytes())
                .await?;
            info!("Strm file updated: {local_path}");
            return Ok(Some(LibraryMediaUpdateKind::Modified));
        }

        self.file_store.ensure_parent_dir(local_path).await?;
        self.file_store
            .write(local_path, expected_url.as_bytes())
            .await?;
        info!("Strm file created: {local_path}");
        Ok(Some(LibraryMediaUpdateKind::Created))
    }

    async fn sync_subtitle_file(
        &self,
        file_id: i64,
        remote_size: u64,
        local_path: &str,
    ) -> AppResult<Option<LibraryMediaUpdateKind>> {
        if let Some(size) = self.file_store.metadata_len_if_exists(local_path).await? {
            if size == remote_size {
                return Ok(None);
            }
            self.remote
                .download_library_file(file_id, local_path)
                .await?;
            info!("Subtitle file updated: {local_path}");
            return Ok(Some(LibraryMediaUpdateKind::Modified));
        }

        self.file_store.ensure_parent_dir(local_path).await?;
        self.remote
            .download_library_file(file_id, local_path)
            .await?;
        info!("Subtitle file created: {local_path}");
        Ok(Some(LibraryMediaUpdateKind::Created))
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

    use crate::application::ports::{
        LocalEntry, MediaDirectoryRecord, NoopLibraryUpdateNotifier,
        library_update::test_support::RecordingLibraryUpdateNotifier,
    };
    use crate::domain::import::LibraryFile;
    use crate::domain::share::FileHash;

    use super::*;

    #[derive(Clone, Default)]
    struct FakeRemote {
        root_ids: Arc<Mutex<HashMap<String, i64>>>,
        dirs: Arc<Mutex<HashMap<i64, Vec<LibraryFile>>>>,
        downloads: Arc<Mutex<Vec<(i64, String)>>>,
        fail_download: Arc<Mutex<bool>>,
        list_hold: Option<Arc<tokio::sync::Mutex<()>>>,
    }

    #[async_trait::async_trait]
    impl LibraryGateway for FakeRemote {
        async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
            Ok(self.root_ids.lock().unwrap().get(path).copied())
        }

        async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>> {
            if let Some(hold) = &self.list_hold {
                let _guard = hold.lock().await;
            }
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

    fn noop_notifier() -> LibraryUpdateNotifierHandle {
        Arc::new(NoopLibraryUpdateNotifier)
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

        let notifier = RecordingLibraryUpdateNotifier::default();
        let service = SyncStrmService::new(
            remote.clone(),
            file_store.clone(),
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
            Arc::new(notifier.clone()),
        );

        let report = service.execute().await.unwrap();
        assert_eq!(
            report,
            SyncReport {
                created: 2,
                modified: 0,
                deleted: 3,
                unchanged: 0,
            }
        );

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
        assert_eq!(
            notifier.batches(),
            vec![vec![
                LibraryMediaUpdate {
                    path: "/local/Movie.2024.1080p.WEB-DL.strm".to_string(),
                    kind: LibraryMediaUpdateKind::Created,
                },
                LibraryMediaUpdate {
                    path: "/local/show/《LV1魔王與獨居廢勇者》#7 (簡中字幕)【Ani-One Asia】.zh-Hans.srt"
                        .to_string(),
                    kind: LibraryMediaUpdateKind::Created,
                },
                LibraryMediaUpdate {
                    path: "/local/show/old.srt".to_string(),
                    kind: LibraryMediaUpdateKind::Deleted,
                },
                LibraryMediaUpdate {
                    path: "/local/stale.strm".to_string(),
                    kind: LibraryMediaUpdateKind::Deleted,
                },
                LibraryMediaUpdate {
                    path: "/local/obsolete".to_string(),
                    kind: LibraryMediaUpdateKind::Deleted,
                },
            ]]
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
            noop_notifier(),
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

        let notifier = RecordingLibraryUpdateNotifier::default();
        let service = SyncStrmService::new(
            remote.clone(),
            file_store.clone(),
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
            Arc::new(notifier.clone()),
        );

        let report = service.execute().await.unwrap();
        assert_eq!(
            report,
            SyncReport {
                created: 0,
                modified: 0,
                deleted: 0,
                unchanged: 2,
            }
        );

        assert!(file_store.writes.lock().unwrap().is_empty());
        assert!(file_store.ensured.lock().unwrap().is_empty());
        assert!(remote.downloads.lock().unwrap().is_empty());
        assert!(notifier.batches().is_empty());
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
            noop_notifier(),
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
            noop_notifier(),
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
            noop_notifier(),
        );

        let error = service.execute().await.unwrap_err();

        assert!(
            matches!(error, AppError::Internal(message) if message.contains("remove dir failed"))
        );
    }
    #[tokio::test]
    async fn execute_notifies_modified_strm_and_subtitle() {
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
            "stale-url".to_string(),
        );
        file_store
            .sizes
            .lock()
            .unwrap()
            .insert("/local/Movie.2024.1080p.WEB-DL.zh.srt".to_string(), 1);

        let notifier = RecordingLibraryUpdateNotifier::default();
        let service = SyncStrmService::new(
            remote,
            file_store,
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
            Arc::new(notifier.clone()),
        );

        let report = service.execute().await.unwrap();
        assert_eq!(
            report,
            SyncReport {
                created: 0,
                modified: 2,
                deleted: 0,
                unchanged: 0,
            }
        );

        assert_eq!(
            notifier.flat_updates(),
            vec![
                LibraryMediaUpdate {
                    path: "/local/Movie.2024.1080p.WEB-DL.strm".to_string(),
                    kind: LibraryMediaUpdateKind::Modified,
                },
                LibraryMediaUpdate {
                    path: "/local/Movie.2024.1080p.WEB-DL.zh.srt".to_string(),
                    kind: LibraryMediaUpdateKind::Modified,
                },
            ]
        );
    }

    #[tokio::test]
    async fn execute_succeeds_when_library_notify_fails() {
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

        let notifier = RecordingLibraryUpdateNotifier::failing();
        let service = SyncStrmService::new(
            remote,
            FakeFileStore::default(),
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
            Arc::new(notifier.clone()),
        );

        let report = service.execute().await.unwrap();
        assert_eq!(
            report,
            SyncReport {
                created: 1,
                modified: 0,
                deleted: 0,
                unchanged: 0,
            }
        );
        assert_eq!(notifier.flat_updates().len(), 1);
    }

    fn movie_service(remote: FakeRemote, file_store: FakeFileStore) -> SyncStrmService {
        SyncStrmService::new(
            remote,
            file_store,
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "https://host/d".to_string(),
            },
            noop_notifier(),
        )
    }

    fn movie_remote() -> FakeRemote {
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
        remote
    }

    async fn wait_until_not_running(controller: &LibrarySyncController) -> LibrarySyncState {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let snapshot = controller.snapshot();
            if !matches!(snapshot, LibrarySyncState::Running { .. }) {
                return snapshot;
            }
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for library sync to finish");
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn controller_rejects_second_start_while_running() {
        let hold = Arc::new(tokio::sync::Mutex::new(()));
        let permit = hold.lock().await;
        let mut remote = movie_remote();
        remote.list_hold = Some(hold.clone());
        let controller =
            LibrarySyncController::new(movie_service(remote, FakeFileStore::default()));

        assert!(matches!(controller.snapshot(), LibrarySyncState::Idle));
        assert!(controller.try_start());
        assert!(matches!(
            controller.snapshot(),
            LibrarySyncState::Running { .. }
        ));
        assert!(!controller.try_start());

        drop(permit);
        let snapshot = wait_until_not_running(&controller).await;
        match snapshot {
            LibrarySyncState::Succeeded { report, .. } => {
                assert_eq!(
                    report,
                    SyncReport {
                        created: 1,
                        modified: 0,
                        deleted: 0,
                        unchanged: 0,
                    }
                );
            }
            other => panic!("expected succeeded, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn controller_records_failure_and_allows_retry() {
        let remote = FakeRemote::default();
        let controller =
            LibrarySyncController::new(movie_service(remote.clone(), FakeFileStore::default()));

        assert!(controller.try_start());
        let snapshot = wait_until_not_running(&controller).await;
        match snapshot {
            LibrarySyncState::Failed { message, .. } => {
                assert!(message.contains("remote path not found"));
            }
            other => panic!("expected failed, got {other:?}"),
        }

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
        assert!(controller.try_start());
        assert!(matches!(
            wait_until_not_running(&controller).await,
            LibrarySyncState::Succeeded { .. }
        ));
    }
}
