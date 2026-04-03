use std::{collections::HashSet, path::Path};

use tracing::info;

use crate::{
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

        self.sync_dir(&remote_path, root_id).await
    }

    async fn sync_dir(&self, path: &str, dir_id: i64) -> AppResult<()> {
        let mut stack = vec![(path.to_string(), dir_id)];
        while let Some((current_path, current_dir_id)) = stack.pop() {
            let files = self.remote.list_dir(current_dir_id).await?;
            let current_local_dir = self.remote_to_local_path(&current_path);
            let mut expected_files_in_dir = HashSet::new();
            let mut expected_sub_dirs = HashSet::new();

            for file in files {
                let file_path = format!("{current_path}/{}", file.file_name);
                let meta = Metadata::parse(&file.file_name);
                if file.is_dir {
                    expected_sub_dirs.insert(self.remote_to_local_path(&file_path));
                    stack.push((file_path, file.file_id));
                } else if meta.is_video() {
                    let local_path = self.remote_to_local_strm_path(&file_path, &meta.extension);
                    expected_files_in_dir.insert(local_path.clone());
                    self.sync_strm_file(&file_path, file.file_id, &local_path)
                        .await?;
                } else if meta.file_type == FileType::Subtitle {
                    let local_path = self.remote_to_local_path(&file_path);
                    expected_files_in_dir.insert(local_path.clone());
                    self.sync_subtitle_file(file.file_id, file.size, &local_path)
                        .await?;
                }
            }

            self.reconcile_local_dir(
                current_local_dir.as_str(),
                &expected_files_in_dir,
                &expected_sub_dirs,
            )
            .await?;
        }
        Ok(())
    }

    fn remote_to_local_strm_path(&self, remote_file_path: &str, extension: &str) -> String {
        self.remote_to_local_path(remote_file_path)
            .trim_end_matches(extension)
            .to_owned()
            + ".strm"
    }

    fn remote_to_local_path(&self, remote_file_path: &str) -> String {
        remote_file_path.replace(
            self.config.remote_path.as_str(),
            self.config.local_path.as_str(),
        )
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

    async fn reconcile_local_dir(
        &self,
        local_dir: &str,
        expected_files_in_dir: &HashSet<String>,
        expected_sub_dirs: &HashSet<String>,
    ) -> AppResult<()> {
        for entry in self.file_store.read_dir(local_dir).await? {
            if entry.is_dir {
                if !expected_sub_dirs.contains(&entry.path) {
                    self.delete_local_dir(entry.path.as_str()).await?;
                }
            } else if !expected_files_in_dir.contains(&entry.path) {
                self.file_store.remove_file(entry.path.as_str()).await?;
                info!("Deleted stale local file: {}", entry.path);
            }
        }
        Ok(())
    }

    async fn delete_local_dir(&self, dir: &str) -> AppResult<()> {
        if Path::new(dir).exists() {
            self.file_store.remove_dir_all(dir).await?;
            info!("Deleted stale local directory: {dir}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_to_local_strm_path_rewrites_prefix_and_extension() {
        let service = SyncStrmService::new(
            (),
            (),
            SyncStrmConfig {
                remote_path: "/remote".to_string(),
                local_path: "/local".to_string(),
                strm_download_url: "http://example.com/d".to_string(),
            },
        );

        let path = service.remote_to_local_strm_path("/remote/show/ep01.mkv", ".mkv");
        assert_eq!(path, "/local/show/ep01.strm");
    }

    impl LibraryRemote for () {
        async fn get_file_id_by_path(&self, _path: &str) -> AppResult<Option<i64>> {
            Ok(None)
        }

        async fn list_dir(&self, _dir_id: i64) -> AppResult<Vec<super::super::ports::RemoteEntry>> {
            Ok(Vec::new())
        }

        async fn download_file(&self, _file_id: i64, _local_path: &str) -> AppResult<()> {
            Ok(())
        }
    }

    impl FileStore for () {
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

        async fn read_dir(&self, _path: &str) -> AppResult<Vec<super::super::ports::LocalEntry>> {
            Ok(Vec::new())
        }

        async fn remove_file(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }

        async fn remove_dir_all(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
    }
}
