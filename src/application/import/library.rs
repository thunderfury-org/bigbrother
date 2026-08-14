use std::collections::HashMap;

use tracing::info;

use crate::application::import_ports::{ImportLocalStore, LibraryGateway};
use crate::domain::{import::inner::MediaFile, share::RawFile};
use crate::error::AppResult;

use super::TransferWorkflow;

impl<L, F> TransferWorkflow<L, F>
where
    L: LibraryGateway,
    F: ImportLocalStore,
{
    pub(super) async fn list_episode_files_in_library(
        &mut self,
        season_dir_id: i64,
    ) -> AppResult<HashMap<u32, Vec<MediaFile>>> {
        let media_files = self.list_media_files_in_library(season_dir_id).await?;

        let grouped_files = media_files
            .into_iter()
            .map(|f| (f.metadata.episode_number.unwrap_or_default(), f))
            .fold(HashMap::new(), |mut acc, (episode, file)| {
                acc.entry(episode).or_insert_with(Vec::new).push(file);
                acc
            });

        Ok(grouped_files)
    }

    pub(super) async fn list_movie_files_in_library(
        &mut self,
        movie_dir_id: i64,
    ) -> AppResult<Vec<MediaFile>> {
        self.list_media_files_in_library(movie_dir_id).await
    }

    async fn list_media_files_in_library(&mut self, dir_id: i64) -> AppResult<Vec<MediaFile>> {
        let files = self.library_gateway.list_library_files(dir_id).await?;

        let mut raw_files = Vec::new();
        for file in &files {
            if file.is_dir {
                continue;
            }

            raw_files.push(RawFile {
                id: Some(file.file_id),
                name: file.file_name.to_owned(),
                hash: file.hash.as_str().into(),
                size: file.size,
                path: "".to_owned(),
            });
        }

        Ok(self
            .metadata_lookup
            .build_media_files(raw_files, Vec::new()))
    }

    pub(super) async fn get_or_create_dir_in_library(&self, path: &str) -> AppResult<i64> {
        info!("Checking if dir {} exists in library", path);
        let file_id = self
            .library_gateway
            .get_library_dir_id_by_path(path)
            .await?;
        match file_id {
            Some(id) => Ok(id),
            None => {
                info!("Dir {} not found in library", path);
                let id = self.library_gateway.mkdir_library_path(path).await?;
                info!("Dir {} created in library, id: {}", path, id);
                Ok(id)
            }
        }
    }
}
