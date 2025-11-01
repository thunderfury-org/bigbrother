use std::{collections::HashMap, hash::Hash};

use reqwest::Url;
use tracing::info;

use super::{ImportSummary, category};
use crate::{
    client::{
        pan123::File,
        tmdb::{self},
    },
    error::AppResult,
    media::{Metadata, Title},
    state::AppState,
};

#[derive(Debug)]
pub struct RawFile {
    pub id: i64,
    pub name: String,
    pub etag: String,
    pub size: u64,
    pub path: String,
}

struct MediaFile {
    metadata: Metadata,
    raw: RawFile,
}

enum Media<'a> {
    Movie {
        detail: tmdb::MovieDetail,
        files: Vec<&'a MediaFile>,
    },
    Tv {
        detail: tmdb::TvDetail,
        // (season, episode) -> files[]
        files: HashMap<u32, HashMap<u32, Vec<&'a MediaFile>>>,
    },
}

pub(super) struct Importer {
    state: AppState,
    tv_info_cache: HashMap<String, Option<tmdb::TvDetail>>,
    movie_info_cache: HashMap<String, Option<tmdb::MovieDetail>>,
    summary: ImportSummary,
    start_time: std::time::Instant,
}

impl Importer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tv_info_cache: HashMap::new(),
            movie_info_cache: HashMap::new(),
            summary: ImportSummary::default(),
            start_time: std::time::Instant::now(),
        }
    }

    pub(super) async fn import_from_share_url(&mut self, url: &Url) -> AppResult<ImportSummary> {
        let share_key = url
            .path_segments()
            .map(|s| s.last().unwrap_or_default())
            .unwrap_or_default();
        let share_password = url
            .query_pairs()
            .find(|(k, _)| k == "pwd")
            .map(|(_, v)| v.to_string())
            .unwrap_or_default();

        let media_files = self.list_files_from_share(share_key, &share_password).await?;
        self.import_media_files(&media_files).await
    }

    async fn import_media_files(&mut self, media_files: &[MediaFile]) -> AppResult<ImportSummary> {
        let medias = self.group_media_files(media_files).await?;
        for media in &medias {
            match media {
                Media::Movie { detail, files } => {
                    self.import_movie(detail, files).await?;
                }
                Media::Tv { detail, files } => {
                    self.import_tv(detail, files).await?;
                }
            }
        }

        self.summary.cost = self.start_time.elapsed();
        Ok(self.summary.clone())
    }

    async fn import_movie(&mut self, detail: &tmdb::MovieDetail, files: &[&MediaFile]) -> AppResult<()> {
        // list existing movies in library
        Ok(())
    }

    async fn import_tv(
        &mut self,
        detail: &tmdb::TvDetail,
        files: &HashMap<u32, HashMap<u32, Vec<&MediaFile>>>,
    ) -> AppResult<()> {
        // list existing episodes in library
        let path = self.get_tv_path_in_library(detail);
        let file_id = self.state.pan123.get_file_id_by_path(path.as_str()).await?;
        if file_id.is_none() {
            info!("tv series {} not found in library, path: {}", detail.name, path);
            self.summary.skipped += 1;
            self.summary.unknown_files.push(path);
            return Ok(());
        }
        info!(
            "tv series {} found in library, file_id: {}, path: {}",
            detail.name,
            file_id.unwrap(),
            path
        );
        Ok(())
    }

    async fn group_media_files<'a>(&mut self, files: &'a [MediaFile]) -> AppResult<Vec<Media<'a>>> {
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

    async fn group_tv_file<'a>(
        &mut self,
        file: &'a MediaFile,
        grouped_files: &mut HashMap<u32, Media<'a>>,
    ) -> AppResult<()> {
        let tv_info = self
            .get_tv_info_from_tmdb(&file.metadata.titles, &file.metadata.year)
            .await?;
        match tv_info {
            Some(tv_info) => {
                let season_number = match file.metadata.season_number {
                    Some(s) => s,
                    None => {
                        if tv_info.number_of_seasons == 1 {
                            1
                        } else {
                            // multi season, but no season number found in file metadata
                            self.summary.skipped += 1;
                            self.summary.unknown_files.push(file.raw.path.to_owned());
                            return Ok(());
                        }
                    }
                };
                let episode_number = match file.metadata.episode_number {
                    Some(e) => e,
                    None => {
                        // episode number not found in file metadata
                        self.summary.skipped += 1;
                        self.summary.unknown_files.push(file.raw.path.to_owned());
                        return Ok(());
                    }
                };
                let entry = grouped_files.entry(tv_info.id).or_insert_with(|| Media::Tv {
                    detail: tv_info,
                    files: HashMap::new(),
                });
                match entry {
                    Media::Tv { files, .. } => {
                        files
                            .entry(season_number)
                            .or_insert_with(HashMap::new)
                            .entry(episode_number)
                            .or_insert_with(Vec::new)
                            .push(file);
                    }
                    _ => {}
                }
            }
            None => {
                self.summary.skipped += 1;
                self.summary.unknown_files.push(file.raw.path.to_owned());
            }
        }

        Ok(())
    }

    async fn group_movie_file<'a>(
        &mut self,
        file: &'a MediaFile,
        grouped_files: &mut HashMap<u32, Media<'a>>,
    ) -> AppResult<()> {
        let movie_info = self
            .get_movie_info_from_tmdb(&file.metadata.titles, &file.metadata.year)
            .await?;
        match movie_info {
            Some(movie_info) => {
                let entry = grouped_files.entry(movie_info.id).or_insert_with(|| Media::Movie {
                    detail: movie_info,
                    files: Vec::new(),
                });
                match entry {
                    Media::Movie { files, .. } => {
                        files.push(file);
                    }
                    _ => {}
                }
            }
            None => {
                self.summary.skipped += 1;
                self.summary.unknown_files.push(file.raw.path.to_owned());
            }
        }

        Ok(())
    }

    async fn list_files_from_share(&mut self, share_key: &str, share_password: &str) -> AppResult<Vec<MediaFile>> {
        let mut all_files = Vec::new();
        let mut stack = vec![(0, String::new())];

        while let Some((parent_id, parent_path)) = stack.pop() {
            let files = self
                .state
                .pan123
                .list_share_file(share_key, share_password, parent_id)
                .await?;

            for file in &files {
                if file.is_dir() {
                    // Directory
                    stack.push((file.file_id, format!("{}/{}", parent_path, file.file_name)));
                } else {
                    // Regular file
                    self.summary.total += 1;
                    let metadata = self.parse_media_metadata(&file.file_name, &parent_path);
                    if metadata.file_type.is_empty() {
                        self.summary.skipped += 1;
                        continue;
                    }

                    all_files.push(MediaFile {
                        metadata,
                        raw: RawFile {
                            id: file.file_id,
                            name: file.file_name.to_owned(),
                            etag: file.etag.to_owned(),
                            size: file.size,
                            path: parent_path.to_owned(),
                        },
                    });
                }
            }
        }

        Ok(all_files)
    }

    fn group_library_files(&self, files: Vec<File>) -> HashMap<u32, HashMap<u32, Vec<MediaFile>>> {
        let mut grouped_files = HashMap::new();

        for file in &files {
            let metadata = self.parse_media_metadata(&file.file_name, "");
            if metadata.file_type.is_empty() {
                continue;
            }

            grouped_files
                .entry(metadata.season_number.unwrap_or_default())
                .or_insert_with(HashMap::new)
                .entry(metadata.episode_number.unwrap_or_default())
                .or_insert_with(Vec::new)
                .push(MediaFile {
                    metadata,
                    raw: RawFile {
                        id: file.file_id,
                        name: file.file_name.to_owned(),
                        etag: file.etag.to_owned(),
                        size: file.size,
                        path: "".to_owned(),
                    },
                });
        }

        grouped_files
    }

    async fn list_files_in_library(&self) -> AppResult<Vec<MediaFile>> {
        Ok(vec![])
    }

    fn parse_media_metadata(&self, name: &str, path: &str) -> Metadata {
        // todo: try to parse metadata from path
        Metadata::from(name)
    }

    async fn get_movie_info_from_tmdb(
        &mut self,
        titles: &Vec<Title>,
        year: &str,
    ) -> AppResult<Option<tmdb::MovieDetail>> {
        for title in titles {
            let cache_key = format!("movie:{}:{}", title.title, year);
            if let Some(movie) = self.movie_info_cache.get(&cache_key) {
                return Ok(movie.clone());
            }
            let movies = self.state.tmdb.search_movie(&title.title, year).await?;
            match movies.len() {
                0 => {
                    self.movie_info_cache.insert(cache_key, None);
                    continue;
                }
                1 => {
                    let movie = self.state.tmdb.get_movie_detail(movies[0].id).await?;
                    self.movie_info_cache.insert(cache_key, movie.clone());
                    return Ok(movie);
                }
                _ => {
                    for movie in &movies {
                        if movie.original_title == title.title || movie.title == title.title {
                            let movie = self.state.tmdb.get_movie_detail(movie.id).await?;
                            self.movie_info_cache.insert(cache_key, movie.clone());
                            return Ok(movie);
                        }
                    }

                    self.movie_info_cache.insert(cache_key, None);
                    continue;
                }
            }
        }
        Ok(None)
    }

    async fn get_tv_info_from_tmdb(&mut self, titles: &Vec<Title>, year: &str) -> AppResult<Option<tmdb::TvDetail>> {
        for title in titles {
            let cache_key = format!("tv:{}:{}", title.title, year);
            if let Some(tv) = self.tv_info_cache.get(&cache_key) {
                return Ok(tv.clone());
            }
            let tvs = self.state.tmdb.search_tv(&title.title, year).await?;
            match tvs.len() {
                0 => {
                    self.tv_info_cache.insert(cache_key, None);
                    continue;
                }
                1 => {
                    let tv = self.state.tmdb.get_tv_detail(tvs[0].id).await?;
                    self.tv_info_cache.insert(cache_key, tv.clone());
                    return Ok(tv);
                }
                _ => {
                    for tv in &tvs {
                        if tv.original_name == title.title || tv.name == title.title {
                            let tv = self.state.tmdb.get_tv_detail(tv.id).await?;
                            self.tv_info_cache.insert(cache_key, tv.clone());
                            return Ok(tv);
                        }
                    }

                    self.tv_info_cache.insert(cache_key, None);
                    continue;
                }
            }
        }
        Ok(None)
    }

    fn get_tv_path_in_library(&self, tv: &tmdb::TvDetail) -> String {
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

    fn get_movie_path_in_library(&self, movie: &tmdb::MovieDetail) -> String {
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

    fn get_year_from_date<'a>(&self, date: &'a str) -> &'a str {
        date.split('-').nth(0).unwrap_or_default()
    }
}
