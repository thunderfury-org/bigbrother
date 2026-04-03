use std::collections::HashMap;

use crate::{
    client::{
        pan115, pan123, pan189,
        tmdb::{self, MovieDetail, TvDetail},
    },
    media::Metadata,
    state::AppState,
};

mod category;
mod group;
mod inner;
pub(super) mod json;
mod library;
mod metadata;
pub(super) mod share;
mod tmdb_info;
mod transfer;

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
pub(crate) struct ImportContext {
    pub pan115: pan115::Client,
    pub pan123: pan123::Client,
    pub pan189: pan189::Client,
    pub tmdb: tmdb::Client,
    pub remote_path: String,
    pub local_path: String,
    pub strm_download_url: String,
}

pub(crate) struct Importer {
    ctx: ImportContext,
    tv_info_cache: HashMap<String, Option<TvDetail>>,
    movie_info_cache: HashMap<String, Option<MovieDetail>>,
    metadata_cache: HashMap<String, Box<Metadata>>,
}

impl Importer {
    pub fn new(state: AppState) -> Self {
        Self {
            ctx: ImportContext {
                pan115: state.client().pan115.clone(),
                pan123: state.client().pan123.clone(),
                pan189: state.client().pan189.clone(),
                tmdb: state.client().tmdb.clone(),
                remote_path: state.config().get_library_config().remote_path.clone(),
                local_path: state.config().get_library_config().local_path.clone(),
                strm_download_url: state
                    .config()
                    .get_media_server_config()
                    .get_strm_download_url(),
            },
            tv_info_cache: HashMap::new(),
            movie_info_cache: HashMap::new(),
            metadata_cache: HashMap::new(),
        }
    }
}
