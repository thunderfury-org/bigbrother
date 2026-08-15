use crate::{
    application::ports::{FileStore, FileStoreHandle},
    domain::library::path_mapping::SyncPathMapper,
    error::AppResult,
};

#[derive(Clone)]
pub struct ImportLocalStore {
    file_store: FileStoreHandle,
    path_mapper: SyncPathMapper,
    remote_path: String,
    strm_download_url: String,
}

impl ImportLocalStore {
    pub fn new(
        file_store: impl FileStore + 'static,
        remote_path: String,
        local_path: String,
        strm_download_url: String,
    ) -> Self {
        Self {
            file_store: std::sync::Arc::new(file_store),
            path_mapper: SyncPathMapper::new(remote_path.clone(), local_path),
            remote_path,
            strm_download_url,
        }
    }

    pub fn remote_library_path(&self) -> &str {
        self.remote_path.as_str()
    }

    pub fn local_path_for_remote(&self, remote_path: &str) -> String {
        self.path_mapper.remote_to_local_path(remote_path)
    }

    pub fn local_strm_path(&self, remote_file_path: &str, extension: &str) -> String {
        self.path_mapper
            .remote_to_local_strm_path(remote_file_path, extension)
    }

    pub async fn write_strm_file(
        &self,
        remote_file_path: &str,
        extension: &str,
        file_id: i64,
    ) -> AppResult<()> {
        let local_file_path = self.local_strm_path(remote_file_path, extension);
        let strm_file_content = self.build_strm_url(remote_file_path, file_id);
        self.file_store
            .ensure_parent_dir(local_file_path.as_str())
            .await?;
        self.file_store
            .write(local_file_path.as_str(), strm_file_content.as_bytes())
            .await
    }

    pub async fn remove_local_file_if_exists(&self, path: &str) -> AppResult<()> {
        self.file_store.remove_file_if_exists(path).await
    }

    pub async fn remove_local_dir_if_exists(&self, path: &str) -> AppResult<()> {
        self.file_store.remove_dir_all_if_exists(path).await
    }

    fn build_strm_url(&self, remote_file_path: &str, file_id: i64) -> String {
        format!(
            "{}{}?file_id={}",
            self.strm_download_url, remote_file_path, file_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{FileStore, LocalEntry};
    use crate::error::AppResult;

    #[derive(Clone, Copy, Default)]
    struct NoopFileStore;

    #[async_trait::async_trait]
    impl FileStore for NoopFileStore {
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
        async fn remove_dir_all_if_exists(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
    }

    fn local_store() -> ImportLocalStore {
        ImportLocalStore::new(
            NoopFileStore,
            "/remote".to_string(),
            "/local".to_string(),
            "http://localhost/d".to_string(),
        )
    }

    #[test]
    fn local_path_rewrites_remote_prefix() {
        let local = local_store().local_path_for_remote("/remote/show/ep01.mkv");

        assert_eq!(local, "/local/show/ep01.mkv");
    }

    #[test]
    fn build_strm_url_uses_configured_prefix() {
        let url = local_store().build_strm_url("/remote/show/ep01.mkv", 42);

        assert_eq!(url, "http://localhost/d/remote/show/ep01.mkv?file_id=42");
    }

    #[tokio::test]
    async fn remove_local_dir_if_exists_ignores_missing_dir() {
        local_store()
            .remove_local_dir_if_exists("/tmp/bigbrother-missing-dir")
            .await
            .unwrap();
    }
}
