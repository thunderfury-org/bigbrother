use std::{collections::HashSet, path::Path};

use tracing::info;

use crate::{
    error::{AppError, AppResult},
    media::{FileType, Metadata},
    state::AppState,
};

pub(super) struct Syncer {
    state: AppState,
}

impl Syncer {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn sync_strm(&self) -> AppResult<()> {
        let remote_path = self.state.config().get_library_config().remote_path.clone();
        let root_id = self
            .state
            .client()
            .pan123
            .get_file_id_by_path(&remote_path)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("remote path not found: {}", remote_path)))?;

        self.sync_dir(&remote_path, root_id).await?;

        Ok(())
    }

    async fn sync_dir(&self, path: &str, dir_id: i64) -> AppResult<()> {
        let mut stack = vec![(path.to_string(), dir_id)];
        while let Some((current_path, current_dir_id)) = stack.pop() {
            let files = self.state.client().pan123.list(current_dir_id).await?;
            let current_local_dir = self.remote_to_local_path(current_path.as_str());
            let mut expected_files_in_dir = HashSet::new();
            let mut expected_sub_dirs = HashSet::new();

            for file in files {
                let file_path = format!("{}/{}", current_path, file.file_name);
                let meta = Metadata::parse(&file.file_name);
                if file.is_dir() {
                    expected_sub_dirs.insert(self.remote_to_local_path(file_path.as_str()));
                    stack.push((file_path, file.file_id));
                } else if meta.is_video() {
                    let local_path = self.remote_to_local_strm_path(&file_path, &meta.extension);
                    expected_files_in_dir.insert(local_path.clone());
                    self.sync_strm_file(&file_path, file.file_id, &local_path).await?;
                } else if meta.file_type == FileType::Subtitle {
                    let local_path = self.remote_to_local_path(&file_path);
                    expected_files_in_dir.insert(local_path.clone());
                    self.sync_subtitle_file(file.file_id, file.size, &local_path).await?;
                }
            }

            self.reconcile_local_dir(current_local_dir.as_str(), &expected_files_in_dir, &expected_sub_dirs)
                .await?;
        }
        Ok(())
    }

    fn remote_to_local_strm_path(&self, remote_file_path: &str, extension: &str) -> String {
        remote_file_path
            .replace(
                self.state.config().get_library_config().remote_path.as_str(),
                self.state.config().get_library_config().local_path.as_str(),
            )
            .trim_end_matches(extension)
            .to_owned()
            + ".strm"
    }

    fn remote_to_local_path(&self, remote_file_path: &str) -> String {
        remote_file_path.replace(
            self.state.config().get_library_config().remote_path.as_str(),
            self.state.config().get_library_config().local_path.as_str(),
        )
    }

    async fn sync_strm_file(&self, remote_file_path: &str, file_id: i64, local_path: &str) -> AppResult<()> {
        let expected_url = format!(
            "{}{}?file_id={}",
            self.state.config().get_media_server_config().get_strm_download_url(),
            remote_file_path,
            file_id
        );

        match tokio::fs::read_to_string(local_path).await {
            Ok(existing) => {
                if existing == expected_url {
                    return Ok(());
                }
                tokio::fs::write(local_path, expected_url.as_bytes()).await?;
                info!("Strm file updated: {}", local_path);
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        tokio::fs::create_dir_all(Path::new(local_path).parent().unwrap()).await?;
        tokio::fs::write(local_path, expected_url.as_bytes()).await?;
        info!("Strm file created: {}", local_path);
        Ok(())
    }

    async fn sync_subtitle_file(&self, file_id: i64, remote_size: u64, local_path: &str) -> AppResult<()> {
        match tokio::fs::metadata(local_path).await {
            Ok(metadata) => {
                if metadata.len() == remote_size {
                    return Ok(());
                }
                self.state.client().pan123.download_file(file_id, local_path).await?;
                info!("Subtitle file updated: {}", local_path);
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        tokio::fs::create_dir_all(Path::new(local_path).parent().unwrap()).await?;
        self.state.client().pan123.download_file(file_id, local_path).await?;
        info!("Subtitle file created: {}", local_path);
        Ok(())
    }

    async fn reconcile_local_dir(
        &self,
        local_dir: &str,
        expected_files_in_dir: &HashSet<String>,
        expected_sub_dirs: &HashSet<String>,
    ) -> AppResult<()> {
        let mut entries = match tokio::fs::read_dir(local_dir).await {
            Ok(e) => e,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(e.into());
                }
                return Ok(());
            }
        };

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let path_str = path.to_string_lossy().to_string();
            if path.is_dir() {
                if !expected_sub_dirs.contains(&path_str) {
                    self.delete_local_dir(path.as_path()).await?;
                }
            } else if !expected_files_in_dir.contains(&path_str) {
                tokio::fs::remove_file(&path).await?;
                info!("Deleted stale local file: {}", path_str);
            }
        }
        Ok(())
    }

    async fn delete_local_dir(&self, dir: &Path) -> AppResult<()> {
        tokio::fs::remove_dir_all(dir).await?;
        info!("Deleted stale local directory: {}", dir.to_string_lossy());
        Ok(())
    }
}
