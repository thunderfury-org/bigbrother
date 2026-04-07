use tracing::info;

use crate::{
    domain::library::{
        path_mapping::SyncPathMapper,
        sync_plan::{LocalNode, PlannedFile, PlannedFileKind, SyncPlan, build_sync_plan},
    },
    error::{AppError, AppResult},
    media::{FileType, Metadata},
};

use super::ports::{FileStore, LibraryRemote};

#[derive(Debug, Clone)]
pub struct SyncStrmConfig {
    pub remote_path: String,
    pub local_path: String,
    pub strm_download_url: String,
}

#[derive(Clone)]
pub struct SyncStrmService<R, F> {
    remote: R,
    file_store: F,
    config: SyncStrmConfig,
}

impl<R, F> SyncStrmService<R, F> {
    pub fn new(remote: R, file_store: F, config: SyncStrmConfig) -> Self {
        Self {
            remote,
            file_store,
            config,
        }
    }
}

impl<R, F> SyncStrmService<R, F>
where
    R: LibraryRemote,
    F: FileStore,
{
    pub async fn execute(&self) -> AppResult<()> {
        let remote_path = self.config.remote_path.clone();
        let root_id = self
            .remote
            .get_file_id_by_path(&remote_path)
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
            let files = self.remote.list_dir(current_dir_id).await?;
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
            self.remote.download_file(file_id, local_path).await?;
            info!("Subtitle file updated: {local_path}");
            return Ok(());
        }

        self.file_store.ensure_parent_dir(local_path).await?;
        self.remote.download_file(file_id, local_path).await?;
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

    use crate::application::ports::{LocalEntry, RemoteEntry};

    use super::*;

    #[derive(Clone, Default)]
    struct FakeRemote {
        root_ids: Arc<Mutex<HashMap<String, i64>>>,
        dirs: Arc<Mutex<HashMap<i64, Vec<RemoteEntry>>>>,
        downloads: Arc<Mutex<Vec<(i64, String)>>>,
    }

    impl LibraryRemote for FakeRemote {
        async fn get_file_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
            Ok(self.root_ids.lock().unwrap().get(path).copied())
        }

        async fn list_dir(&self, dir_id: i64) -> AppResult<Vec<RemoteEntry>> {
            Ok(self
                .dirs
                .lock()
                .unwrap()
                .get(&dir_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn download_file(&self, file_id: i64, local_path: &str) -> AppResult<()> {
            self.downloads
                .lock()
                .unwrap()
                .push((file_id, local_path.to_string()));
            Ok(())
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
    }

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
            self.removed_dirs.lock().unwrap().push(path.to_string());
            Ok(())
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
                RemoteEntry {
                    file_id: 2,
                    file_name: "show".to_string(),
                    is_dir: true,
                    size: 0,
                },
                RemoteEntry {
                    file_id: 3,
                    file_name: "Movie.2024.1080p.WEB-DL.mkv".to_string(),
                    is_dir: false,
                    size: 100,
                },
            ],
        );
        remote.dirs.lock().unwrap().insert(
            2,
            vec![RemoteEntry {
                file_id: 4,
                file_name: "《LV1魔王與獨居廢勇者》#7 (簡中字幕)【Ani-One Asia】.zh-Hans.srt"
                    .to_string(),
                is_dir: false,
                size: 8,
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
}
