use crate::{
    application::ports::{
        DownloadUrlError, DownloadUrlResult, DownloadUrlSource, LibraryRemote, RemoteEntry,
    },
    error::AppResult,
};

use super::{RequestError, pan123};

#[derive(Clone)]
pub struct Pan123LibraryRemote {
    client: pan123::Client,
}

impl Pan123LibraryRemote {
    pub fn new(client: pan123::Client) -> Self {
        Self { client }
    }
}

impl LibraryRemote for Pan123LibraryRemote {
    async fn get_file_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
        Ok(self.client.get_file_id_by_path(path).await?)
    }

    async fn list_dir(&self, dir_id: i64) -> AppResult<Vec<RemoteEntry>> {
        Ok(self
            .client
            .list(dir_id)
            .await?
            .into_iter()
            .map(|file| {
                let is_dir = file.is_dir();
                RemoteEntry {
                    file_id: file.file_id,
                    file_name: file.file_name,
                    is_dir,
                    size: file.size,
                }
            })
            .collect())
    }

    async fn download_file(&self, file_id: i64, local_path: &str) -> AppResult<()> {
        self.client.download_file(file_id, local_path).await?;
        Ok(())
    }
}

impl DownloadUrlSource for Pan123LibraryRemote {
    async fn get_download_url(&self, file_id: i64) -> DownloadUrlResult<String> {
        self.client
            .get_download_url(file_id)
            .await
            .map_err(map_download_url_error)
    }
}

fn map_download_url_error(err: RequestError) -> DownloadUrlError {
    match err {
        RequestError::Unauthorized => DownloadUrlError::Unauthorized,
        RequestError::NotFound(message) => DownloadUrlError::NotFound(message),
        RequestError::AlreadyExists => DownloadUrlError::Error("already exists".to_string()),
        RequestError::TooManyRequests => DownloadUrlError::Error("too many requests".to_string()),
        RequestError::Error(message) => DownloadUrlError::Error(message),
    }
}
