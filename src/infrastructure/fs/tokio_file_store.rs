use std::path::Path;

use crate::{
    application::ports::{FileStore, LocalEntry},
    error::AppResult,
};

#[derive(Clone, Copy, Default)]
pub struct TokioFileStore;

#[async_trait::async_trait]
impl FileStore for TokioFileStore {
    async fn read_to_string_if_exists(&self, path: &str) -> AppResult<Option<String>> {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => Ok(Some(content)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    async fn write(&self, path: &str, bytes: &[u8]) -> AppResult<()> {
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    async fn metadata_len_if_exists(&self, path: &str) -> AppResult<Option<u64>> {
        match tokio::fs::metadata(path).await {
            Ok(metadata) => Ok(Some(metadata.len())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    async fn ensure_parent_dir(&self, path: &str) -> AppResult<()> {
        let parent = Path::new(path).parent().unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(parent).await?;
        Ok(())
    }

    async fn read_dir(&self, path: &str) -> AppResult<Vec<LocalEntry>> {
        let mut dir = match tokio::fs::read_dir(path).await {
            Ok(dir) => dir,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err.into()),
        };
        let mut entries = Vec::new();

        while let Some(entry) = dir.next_entry().await? {
            let entry_path = entry.path();
            let metadata = entry.metadata().await?;
            entries.push(LocalEntry {
                path: entry_path.to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
            });
        }

        Ok(entries)
    }

    async fn remove_file(&self, path: &str) -> AppResult<()> {
        tokio::fs::remove_file(path).await?;
        Ok(())
    }

    async fn remove_dir_all(&self, path: &str) -> AppResult<()> {
        tokio::fs::remove_dir_all(Path::new(path)).await?;
        Ok(())
    }
}
