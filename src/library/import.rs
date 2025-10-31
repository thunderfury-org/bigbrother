use std::collections::HashMap;

use reqwest::Url;

use crate::{
    client::{
        pan123::File,
        tmdb::{self, Genre},
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

#[derive(Default)]
struct GroupedMediaFiles<'a> {
    is_tv: bool,
    tmdb_id: String,
    title: String,
    year: String,
    genres: Vec<Genre>,
    origin_country: Vec<String>,

    movie_files: Vec<&'a MediaFile>,
    // (season, episode) -> files[]
    episode_files: HashMap<(u32, u32), Vec<&'a MediaFile>>,
}

#[derive(Debug, Default, Clone)]
pub struct ImportSummary {
    pub success: u32,
    pub failed: u32,
    pub skipped: u32,
    pub total: u32,
    pub total_size: u64,
    pub cost: std::time::Duration,
    pub unknown_files: Vec<String>,
}

pub async fn import_from_share_url(state: &AppState, url: &Url) -> AppResult<ImportSummary> {
    Importer::new(state.clone()).import_from_share_url(url).await
}

pub async fn import_from_fslink(state: &AppState, files: Vec<RawFile>) -> AppResult<ImportSummary> {
    // Placeholder implementation
    Ok(ImportSummary::default())
}

pub async fn import_from_remote_dir(state: &AppState, dir: &str) -> AppResult<ImportSummary> {
    // Placeholder implementation
    Ok(ImportSummary::default())
}

struct Importer {
    state: AppState,
    tv_info_cache: HashMap<String, Option<tmdb::TvDetail>>,
    movie_info_cache: HashMap<String, Option<tmdb::MovieDetail>>,
    summary: ImportSummary,
}

impl Importer {
    // add new for importer
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tv_info_cache: HashMap::new(),
            movie_info_cache: HashMap::new(),
            summary: ImportSummary::default(),
        }
    }

    async fn import_from_share_url(&mut self, url: &Url) -> AppResult<ImportSummary> {
        let share_key = url
            .path_segments()
            .map(|s| s.last().unwrap_or_default())
            .unwrap_or_default();
        let share_password = url
            .query_pairs()
            .find(|(k, _)| k == "pwd")
            .map(|(_, v)| v.to_string())
            .unwrap_or_default();

        let media_files = self.list_files_in_share(share_key, &share_password).await?;
        let grouped_media_files = self.group_share_files(&media_files).await?;
        for grouped_file in grouped_media_files {
            if grouped_file.is_tv {
                // tv
                self.import_tv_show(&grouped_file).await?;
            } else {
                // movie
            }
        }

        Ok(self.summary.clone())
    }

    async fn import_tv_show(&mut self, grouped_file: &GroupedMediaFiles<'_>) -> AppResult<()> {
        // list existing episodes in library
        Ok(())
    }

    async fn group_share_files<'a>(&mut self, files: &'a [MediaFile]) -> AppResult<Vec<GroupedMediaFiles<'a>>> {
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
        grouped_files: &mut HashMap<String, GroupedMediaFiles<'a>>,
    ) -> AppResult<()> {
        let tv_info = self
            .get_tv_info_from_tmdb(&file.metadata.titles, &file.metadata.year)
            .await?;
        match tv_info {
            Some(tv_info) => {
                grouped_files
                    .entry(tv_info.id.to_string())
                    .or_insert_with(|| GroupedMediaFiles {
                        is_tv: true,
                        tmdb_id: tv_info.id.to_string(),
                        title: tv_info.name.to_owned(),
                        year: tv_info.first_air_date.split('-').next().unwrap_or_default().to_owned(),
                        genres: tv_info.genres,
                        origin_country: tv_info.original_country,
                        movie_files: Vec::new(),
                        episode_files: HashMap::new(),
                    })
                    .episode_files
                    .entry((
                        file.metadata.season_number.unwrap_or_default(),
                        file.metadata.episode_number.unwrap_or_default(),
                    ))
                    .or_insert_with(Vec::new)
                    .push(file);
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
        grouped_files: &mut HashMap<String, GroupedMediaFiles<'a>>,
    ) -> AppResult<()> {
        let movie_info = self
            .get_movie_info_from_tmdb(&file.metadata.titles, &file.metadata.year)
            .await?;
        match movie_info {
            Some(movie_info) => {
                grouped_files
                    .entry(movie_info.id.to_string())
                    .or_insert_with(|| GroupedMediaFiles {
                        is_tv: false,
                        tmdb_id: movie_info.id.to_string(),
                        title: movie_info.title.to_owned(),
                        year: movie_info.release_date.split('-').next().unwrap_or_default().to_owned(),
                        genres: movie_info.genres,
                        origin_country: movie_info.origin_country,
                        movie_files: Vec::new(),
                        episode_files: HashMap::new(),
                    })
                    .movie_files
                    .push(file);
            }
            None => {
                self.summary.skipped += 1;
                self.summary.unknown_files.push(file.raw.path.to_owned());
            }
        }

        Ok(())
    }

    async fn list_files_in_share(&mut self, share_key: &str, share_password: &str) -> AppResult<Vec<MediaFile>> {
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
}
