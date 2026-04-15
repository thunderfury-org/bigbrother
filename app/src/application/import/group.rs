use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog};
use crate::domain::import::{
    inner::{Media, MediaFile},
    policy::{insert_movie_media, insert_tv_media, resolve_tv_episode_slot},
};
use std::collections::HashMap;

use super::TransferWorkflow;
use crate::error::AppResult;

impl<L, M, F> TransferWorkflow<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    /// 按 tmdb 信息分组媒体文件，分类为 TV 和 Movie
    pub(super) async fn group_media_files<'a>(
        &mut self,
        files: &'a [MediaFile],
    ) -> AppResult<Vec<Media<'a>>> {
        // group files by tmdb_id
        let mut grouped_files = HashMap::new();
        for file in files {
            if file.metadata.episode_number.is_some() {
                // tv
                self.group_tv_file(file, &mut grouped_files).await?;
            } else {
                // movie
                self.group_movie_file(file, &mut grouped_files).await?;
            }
        }
        Ok(grouped_files.into_values().collect())
    }

    /// 按 tmdb_id 分组 TV 文件
    async fn group_tv_file<'a>(
        &mut self,
        file: &'a MediaFile,
        grouped_files: &mut HashMap<u32, Media<'a>>,
    ) -> AppResult<()> {
        // 从 tmdb 获取 tv 详情
        let tv_info = self.tmdb_lookup.get_tv_info(&file.metadata).await?;
        match tv_info {
            Some(tv_info) => {
                let Some((season_number, episode_number)) = resolve_tv_episode_slot(file, &tv_info)
                else {
                    return Ok(());
                };
                insert_tv_media(grouped_files, tv_info, season_number, episode_number, file);
            }
            None => tracing::info!(
                "No tv found in tmdb for file: {}, path: {}",
                file.video.name,
                file.video.path
            ),
        }

        Ok(())
    }

    /// 按 tmdb_id 分组 Movie 文件
    async fn group_movie_file<'a>(
        &mut self,
        file: &'a MediaFile,
        grouped_files: &mut HashMap<u32, Media<'a>>,
    ) -> AppResult<()> {
        // 从 tmdb 获取 movie 详情
        let movie_info = self.tmdb_lookup.get_movie_info(&file.metadata).await?;
        match movie_info {
            Some(movie_info) => {
                insert_movie_media(grouped_files, movie_info, file);
            }
            None => tracing::info!(
                "No movie found in tmdb for file: {}, path: {}",
                file.video.name,
                file.video.path
            ),
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "group/tests.rs"]
mod tests;
