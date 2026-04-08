use std::collections::HashMap;

use crate::domain::media::Metadata;

mod category;
mod group;
mod inner;
pub(super) mod json;
mod library;
mod metadata;
mod model;
pub(crate) mod remote;
pub(super) mod share;
mod tmdb_info;
mod transfer;

pub(crate) use model::{
    Genre, LibraryFile, MovieDetail, Pan115FileEntry, Pan189File, Pan189Folder, Pan189ShareInfo,
    SearchMovieResult, SearchTvResult, Season, TvDetail,
};
pub(crate) use remote::ImportClient;

#[derive(Debug)]
pub enum ImportedMedia {
    Movie {
        title: String,
        year: String,
        size: u64,
        cost: std::time::Duration,
        has_failed: bool,
    },
    Tv {
        name: String,
        year: String,
        season: u32,
        episodes: Vec<u32>,
        missing_episodes: Vec<u32>,
        max_episode_number: u32,
        total_size: u64,
        number_of_episodes: u32,
        cost: std::time::Duration,
        _has_failed: bool,
    },
}

#[derive(Clone)]
pub(crate) struct ImportPathConfig {
    pub(crate) remote_path: String,
    pub(crate) local_path: String,
    pub(crate) strm_download_url: String,
}

pub(crate) struct Importer<C> {
    remote: remote::ImportRemote<C>,
    tv_info_cache: HashMap<String, Option<TvDetail>>,
    movie_info_cache: HashMap<String, Option<MovieDetail>>,
    metadata_cache: HashMap<String, Box<Metadata>>,
}

impl<C> Importer<C>
where
    C: ImportClient,
{
    pub(crate) fn new(client: C, paths: ImportPathConfig) -> Self {
        Self {
            remote: remote::ImportRemote::new(client, paths),
            tv_info_cache: HashMap::new(),
            movie_info_cache: HashMap::new(),
            metadata_cache: HashMap::new(),
        }
    }
}

impl ImportPathConfig {
    pub(crate) fn new(remote_path: String, local_path: String, strm_download_url: String) -> Self {
        Self {
            remote_path,
            local_path,
            strm_download_url,
        }
    }
}
