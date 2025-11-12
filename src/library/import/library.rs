use std::collections::HashMap;

use tracing::info;

use super::{
    Importer, category,
    inner::{MediaFile, RawFile},
};
use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    error::AppResult,
};

impl Importer {
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

    pub(super) async fn list_movie_files_in_library(&mut self, movie_dir_id: i64) -> AppResult<Vec<MediaFile>> {
        self.list_media_files_in_library(movie_dir_id).await
    }

    async fn list_media_files_in_library(&mut self, dir_id: i64) -> AppResult<Vec<MediaFile>> {
        let files = self.state.pan123.list(dir_id).await?;

        let mut raw_files = Vec::new();
        for file in &files {
            if file.is_dir() {
                continue;
            }

            let metadata = self.parse_media_metadata(&file.file_name, "");
            if metadata.unknown_type() {
                continue;
            }

            raw_files.push((
                metadata,
                RawFile {
                    id: Some(file.file_id),
                    name: file.file_name.to_owned(),
                    etag: file.etag.to_owned(),
                    size: file.size,
                    path: "".to_owned(),
                },
            ));
        }

        Ok(self.group_video_and_subtitle_files(raw_files))
    }

    pub(super) async fn get_or_create_dir_in_library(&self, path: &str) -> AppResult<i64> {
        let file_id = self.state.pan123.get_file_id_by_path(path).await?;
        match file_id {
            Some(id) => Ok(id),
            None => {
                info!("Dir {} not found in library", path);
                // create in library
                let id = self.state.pan123.mkdir_by_path(path).await?;
                info!("Dir {} created in library, id: {}", path, id);
                Ok(id)
            }
        }
    }

    pub(super) fn get_tv_path_in_library(&self, tv: &TvDetail) -> String {
        format!(
            "{}/{}/{}/{} ({}) {{tmdb-{}}}",
            self.state.config.get_library_config().remote_path,
            category::get_tv_category(&tv.genres),
            category::get_subcategory(&tv.origin_country),
            tv.name,
            self.get_year_from_date(tv.first_air_date.as_str()),
            tv.id
        )
    }

    pub(super) fn get_movie_path_in_library(&self, movie: &MovieDetail) -> String {
        format!(
            "{}/{}/{}/{} ({}) {{tmdb-{}}}",
            self.state.config.get_library_config().remote_path,
            category::CATEGORY_MOVIE,
            category::get_subcategory(&movie.origin_country),
            movie.title,
            self.get_year_from_date(movie.release_date.as_str()),
            movie.id
        )
    }

    pub(super) fn get_tv_base_name(&self, tv: &TvDetail) -> String {
        format!(
            "{} ({}) {{tmdb-{}}}",
            tv.name,
            self.get_year_from_date(tv.first_air_date.as_str()),
            tv.id
        )
    }

    pub(super) fn get_movie_base_name(&self, movie: &MovieDetail) -> String {
        format!(
            "{} ({}) {{tmdb-{}}}",
            movie.title,
            self.get_year_from_date(movie.release_date.as_str()),
            movie.id
        )
    }

    pub(super) fn get_year_from_date<'a>(&self, date: &'a str) -> &'a str {
        date.split('-').nth(0).unwrap_or_default()
    }
}
