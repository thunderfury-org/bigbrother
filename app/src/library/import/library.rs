use std::collections::HashMap;

use tracing::info;

use super::{
    Importer, category,
    group::group_video_and_subtitle_files,
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
        let files = self.state.pan123().list(dir_id).await?;

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
                    etag: file.etag.as_str().into(),
                    size: file.size,
                    path: "".to_owned(),
                },
            ));
        }

        Ok(group_video_and_subtitle_files(raw_files))
    }

    pub(super) async fn get_or_create_dir_in_library(&self, path: &str) -> AppResult<i64> {
        let file_id = self.state.pan123().get_file_id_by_path(path).await?;
        match file_id {
            Some(id) => Ok(id),
            None => {
                info!("Dir {} not found in library", path);
                // create in library
                let id = self.state.pan123().mkdir_by_path(path).await?;
                info!("Dir {} created in library, id: {}", path, id);
                Ok(id)
            }
        }
    }
}

pub(super) fn get_tv_path_in_library(remote_path: &str, tv: &TvDetail) -> String {
    format!(
        "{}/{}/{}/{}",
        remote_path,
        category::get_tv_category(&tv.genres),
        category::get_subcategory(&tv.origin_country),
        get_tv_base_name(tv)
    )
}

pub(super) fn get_movie_path_in_library(remote_path: &str, movie: &MovieDetail) -> String {
    format!(
        "{}/{}/{}/{}",
        remote_path,
        category::CATEGORY_MOVIE,
        category::get_subcategory(&movie.origin_country),
        get_movie_base_name(movie)
    )
}

pub(super) fn get_tv_base_name(tv: &TvDetail) -> String {
    format!(
        "{} ({}) {{tmdb-{}}}",
        tv.name,
        get_year_from_date(tv.first_air_date.as_str()),
        tv.id
    )
}

pub(super) fn get_movie_base_name(movie: &MovieDetail) -> String {
    format!(
        "{} ({}) {{tmdb-{}}}",
        movie.title,
        get_year_from_date(movie.release_date.as_str()),
        movie.id
    )
}

pub(super) fn get_year_from_date(date: &str) -> &str {
    date.split('-').next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tmdb::Genre;

    fn create_test_tv(
        id: u32,
        name: &str,
        first_air_date: &str,
        genres: Vec<Genre>,
        origin_country: Vec<String>,
    ) -> TvDetail {
        TvDetail {
            id,
            name: name.to_string(),
            first_air_date: first_air_date.to_string(),
            number_of_episodes: 10,
            number_of_seasons: 1,
            origin_country,
            original_language: "en".to_string(),
            original_name: name.to_string(),
            genres,
            seasons: vec![],
        }
    }

    fn create_test_movie(
        id: u32,
        title: &str,
        release_date: &str,
        genres: Vec<Genre>,
        origin_country: Vec<String>,
    ) -> MovieDetail {
        MovieDetail {
            id,
            title: title.to_string(),
            adult: false,
            genres,
            original_language: "en".to_string(),
            original_title: title.to_string(),
            origin_country,
            release_date: release_date.to_string(),
        }
    }

    #[test]
    fn test_get_year_from_date() {
        assert_eq!(get_year_from_date("2024-01-15"), "2024");
        assert_eq!(get_year_from_date("2020-12-31"), "2020");
        assert_eq!(get_year_from_date("1999-06-01"), "1999");
        assert_eq!(get_year_from_date("2024"), "2024");
        assert_eq!(get_year_from_date(""), "");
        assert_eq!(get_year_from_date("invalid-date"), "invalid");
    }

    #[test]
    fn test_get_tv_base_name() {
        let tv = create_test_tv(12345, "Breaking Bad", "2008-01-20", vec![], vec![]);
        assert_eq!(get_tv_base_name(&tv), "Breaking Bad (2008) {tmdb-12345}");

        let tv_no_date = create_test_tv(67890, "The Office", "", vec![], vec![]);
        assert_eq!(get_tv_base_name(&tv_no_date), "The Office () {tmdb-67890}");

        let tv_chinese = create_test_tv(111, "三体", "2023-03-21", vec![], vec![]);
        assert_eq!(get_tv_base_name(&tv_chinese), "三体 (2023) {tmdb-111}");
    }

    #[test]
    fn test_get_movie_base_name() {
        let movie = create_test_movie(98765, "The Matrix", "1999-03-31", vec![], vec![]);
        assert_eq!(get_movie_base_name(&movie), "The Matrix (1999) {tmdb-98765}");

        let movie_no_date = create_test_movie(11111, "Unknown", "", vec![], vec![]);
        assert_eq!(get_movie_base_name(&movie_no_date), "Unknown () {tmdb-11111}");

        let movie_special_chars = create_test_movie(22222, "Star Wars: Episode IV", "1977-05-25", vec![], vec![]);
        assert_eq!(
            get_movie_base_name(&movie_special_chars),
            "Star Wars: Episode IV (1977) {tmdb-22222}"
        );
    }

    #[test]
    fn test_get_tv_path_in_library() {
        // Test with animation genre and Chinese origin
        let genres = vec![Genre {
            id: 16,
            name: "Animation".to_string(),
        }];
        let tv = create_test_tv(12345, "进击的巨人", "2013-04-07", genres, vec!["JP".to_string()]);
        let path = get_tv_path_in_library("/media", &tv);
        assert_eq!(path, "/media/动漫/日韩/进击的巨人 (2013) {tmdb-12345}");

        // Test with regular TV series (no special genre) and US origin
        let tv_regular = create_test_tv(67890, "Friends", "1994-09-22", vec![], vec!["US".to_string()]);
        let path = get_tv_path_in_library("/library", &tv_regular);
        assert_eq!(path, "/library/电视剧/欧美/Friends (1994) {tmdb-67890}");

        // Test with documentary genre and Chinese origin
        let genres_doc = vec![Genre {
            id: 99,
            name: "Documentary".to_string(),
        }];
        let tv_doc = create_test_tv(111, "舌尖上的中国", "2012-05-14", genres_doc, vec!["CN".to_string()]);
        let path = get_tv_path_in_library("/data", &tv_doc);
        assert_eq!(path, "/data/纪录片/国产/舌尖上的中国 (2012) {tmdb-111}");

        // Test with variety show (reality genre) and unknown origin country
        let genres_variety = vec![Genre {
            id: 10764,
            name: "Reality".to_string(),
        }];
        let tv_variety = create_test_tv(222, "Running Man", "2010-07-11", genres_variety, vec!["XX".to_string()]);
        let path = get_tv_path_in_library("/shows", &tv_variety);
        assert_eq!(path, "/shows/综艺/其它/Running Man (2010) {tmdb-222}");
    }

    #[test]
    fn test_get_movie_path_in_library() {
        // Test with US origin
        let movie = create_test_movie(98765, "Inception", "2010-07-16", vec![], vec!["US".to_string()]);
        let path = get_movie_path_in_library("/media", &movie);
        assert_eq!(path, "/media/电影/欧美/Inception (2010) {tmdb-98765}");

        // Test with Chinese origin
        let movie_cn = create_test_movie(11111, "流浪地球", "2019-02-05", vec![], vec!["CN".to_string()]);
        let path = get_movie_path_in_library("/library", &movie_cn);
        assert_eq!(path, "/library/电影/国产/流浪地球 (2019) {tmdb-11111}");

        // Test with Japanese origin
        let movie_jp = create_test_movie(22222, "君の名は", "2016-08-26", vec![], vec!["JP".to_string()]);
        let path = get_movie_path_in_library("/movies", &movie_jp);
        assert_eq!(path, "/movies/电影/日韩/君の名は (2016) {tmdb-22222}");

        // Test with unknown origin country
        let movie_other = create_test_movie(33333, "Some Movie", "2020-01-01", vec![], vec!["ZZ".to_string()]);
        let path = get_movie_path_in_library("/data", &movie_other);
        assert_eq!(path, "/data/电影/其它/Some Movie (2020) {tmdb-33333}");

        // Test with multiple origin countries (should use first match)
        let movie_multi = create_test_movie(
            44444,
            "Co-Production",
            "2021-06-15",
            vec![],
            vec!["FR".to_string(), "US".to_string()],
        );
        let path = get_movie_path_in_library("/share", &movie_multi);
        assert_eq!(path, "/share/电影/欧美/Co-Production (2021) {tmdb-44444}");
    }

    #[test]
    fn test_get_tv_path_with_multiple_genres() {
        // Test that genre priority is respected (first matching genre wins)
        let genres = vec![
            Genre {
                id: 10764,
                name: "Reality".to_string(),
            },
            Genre {
                id: 99,
                name: "Documentary".to_string(),
            },
        ];
        let tv = create_test_tv(333, "Reality Doc", "2020-01-01", genres, vec!["US".to_string()]);
        let path = get_tv_path_in_library("/media", &tv);
        // Should be 综艺 (variety) because reality genre comes first
        assert_eq!(path, "/media/综艺/欧美/Reality Doc (2020) {tmdb-333}");
    }

    #[test]
    fn test_paths_with_different_remote_paths() {
        let movie = create_test_movie(123, "Test Movie", "2020-01-01", vec![], vec!["US".to_string()]);

        let path1 = get_movie_path_in_library("/mnt/media", &movie);
        assert_eq!(path1, "/mnt/media/电影/欧美/Test Movie (2020) {tmdb-123}");

        let path2 = get_movie_path_in_library("/data/library", &movie);
        assert_eq!(path2, "/data/library/电影/欧美/Test Movie (2020) {tmdb-123}");

        let path3 = get_movie_path_in_library(".", &movie);
        assert_eq!(path3, "./电影/欧美/Test Movie (2020) {tmdb-123}");
    }
}
