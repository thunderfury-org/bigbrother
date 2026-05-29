use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, TitleExtractor,
};
use crate::domain::import::{
    inner::{Media, MediaFile},
    policy::{insert_movie_media, insert_tv_media, resolve_tv_episode_slot},
};
use std::collections::HashMap;

use super::TransferWorkflow;
use crate::error::AppResult;

impl<L, M, F, T> TransferWorkflow<L, M, F, T>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
    T: TitleExtractor,
{
    /// 按 tmdb 信息分组媒体文件，分类为 TV 和 Movie，同时返回未匹配的文件
    pub(super) async fn group_media_files<'a>(
        &mut self,
        files: &'a [MediaFile],
    ) -> AppResult<(Vec<Media<'a>>, Vec<(&'a str, &'a str)>)> {
        let mut grouped_files = HashMap::new();
        let mut unmatched = Vec::new();

        for file in files {
            if file.metadata.is_tv_episode() {
                if self
                    .group_tv_file(file, &mut grouped_files)
                    .await?
                    .is_some()
                {
                    unmatched.push((file.video.name.as_str(), file.video.path.as_str()));
                }
            } else if self
                .group_movie_file(file, &mut grouped_files)
                .await?
                .is_some()
            {
                unmatched.push((file.video.name.as_str(), file.video.path.as_str()));
            }
        }
        Ok((grouped_files.into_values().collect(), unmatched))
    }

    /// 按 tmdb_id 分组 TV 文件，返回 Some(()) 表示未匹配
    async fn group_tv_file<'a>(
        &mut self,
        file: &'a MediaFile,
        grouped_files: &mut HashMap<u32, Media<'a>>,
    ) -> AppResult<Option<()>> {
        let tv_info = self
            .tmdb_lookup
            .get_tv_info(&file.metadata, &file.descriptions)
            .await?;
        match tv_info {
            Some(tv_info) => {
                let Some((season_number, episode_number)) = resolve_tv_episode_slot(file, &tv_info)
                else {
                    return Ok(Some(()));
                };
                insert_tv_media(grouped_files, tv_info, season_number, episode_number, file);
                Ok(None)
            }
            None => Ok(Some(())),
        }
    }

    /// 按 tmdb_id 分组 Movie 文件，返回 Some(()) 表示未匹配
    async fn group_movie_file<'a>(
        &mut self,
        file: &'a MediaFile,
        grouped_files: &mut HashMap<u32, Media<'a>>,
    ) -> AppResult<Option<()>> {
        let movie_info = self
            .tmdb_lookup
            .get_movie_info(&file.metadata, &file.descriptions)
            .await?;
        match movie_info {
            Some(movie_info) => {
                insert_movie_media(grouped_files, movie_info, file);
                Ok(None)
            }
            None => Ok(Some(())),
        }
    }
}

#[cfg(test)]
#[path = "group/tests.rs"]
mod tests;
