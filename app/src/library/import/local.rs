use std::{io, path::Path};

use crate::{
    domain::library::path_mapping::SyncPathMapper,
    error::{AppError, AppResult},
};

use super::ImportPathConfig;

#[derive(Clone)]
pub(super) struct ImportLocalStore {
    strm_download_url: String,
    path_mapper: SyncPathMapper,
}

impl ImportLocalStore {
    pub(super) fn new(paths: ImportPathConfig) -> Self {
        Self {
            strm_download_url: paths.strm_download_url,
            path_mapper: SyncPathMapper::new(paths.remote_path, paths.local_path),
        }
    }

    pub(super) fn local_path_for_remote(&self, remote_path: &str) -> String {
        self.path_mapper.remote_to_local_path(remote_path)
    }

    pub(super) fn build_strm_url(&self, remote_file_path: &str, file_id: i64) -> String {
        format!(
            "{}{}?file_id={}",
            self.strm_download_url, remote_file_path, file_id
        )
    }

    pub(super) fn local_strm_path(&self, remote_file_path: &str, extension: &str) -> String {
        self.path_mapper
            .remote_to_local_strm_path(remote_file_path, extension)
    }

    pub(super) async fn write_strm_file(
        &self,
        remote_file_path: &str,
        extension: &str,
        file_id: i64,
    ) -> AppResult<()> {
        let local_file_path = self.local_strm_path(remote_file_path, extension);
        let strm_file_content = self.build_strm_url(remote_file_path, file_id);

        tokio::fs::create_dir_all(Path::new(&local_file_path).parent().unwrap()).await?;
        tokio::fs::write(local_file_path.as_str(), strm_file_content.as_bytes()).await?;
        Ok(())
    }

    pub(super) async fn remove_local_file_if_exists(&self, path: &str) -> AppResult<()> {
        if let Err(err) = tokio::fs::remove_file(path).await
            && err.kind() != io::ErrorKind::NotFound
        {
            return Err(AppError::Internal(format!(
                "Failed to delete local file, error: {}",
                err
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_store() -> ImportLocalStore {
        ImportLocalStore::new(ImportPathConfig::new(
            "/remote".to_string(),
            "/local".to_string(),
            "http://localhost/d".to_string(),
        ))
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
}
