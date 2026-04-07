use std::collections::HashMap;

use crate::{
    client::{
        pan115, pan123, pan189,
        tmdb::{self, MovieDetail, TvDetail},
    },
    media::Metadata,
};

mod category;
mod group;
mod inner;
pub(super) mod json;
mod library;
mod metadata;
mod remote;
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
pub(crate) struct ImportClients {
    pan115: pan115::Client,
    pan123: pan123::Client,
    pan189: pan189::Client,
    tmdb: tmdb::Client,
}

#[derive(Clone)]
pub(crate) struct ImportPathConfig {
    remote_path: String,
    local_path: String,
    strm_download_url: String,
}

#[derive(Clone)]
pub(crate) struct ImportContext {
    clients: ImportClients,
    paths: ImportPathConfig,
}

pub(crate) struct Importer {
    remote: remote::ImportRemote,
    tv_info_cache: HashMap<String, Option<TvDetail>>,
    movie_info_cache: HashMap<String, Option<MovieDetail>>,
    metadata_cache: HashMap<String, Box<Metadata>>,
}

impl Importer {
    pub(crate) fn from_context(ctx: ImportContext) -> Self {
        Self {
            remote: remote::ImportRemote::new(ctx),
            tv_info_cache: HashMap::new(),
            movie_info_cache: HashMap::new(),
            metadata_cache: HashMap::new(),
        }
    }
}

impl ImportContext {
    pub(crate) fn new(
        pan115: pan115::Client,
        pan123: pan123::Client,
        pan189: pan189::Client,
        tmdb: tmdb::Client,
        remote_path: String,
        local_path: String,
        strm_download_url: String,
    ) -> Self {
        Self::from_parts(
            ImportClients {
                pan115,
                pan123,
                pan189,
                tmdb,
            },
            ImportPathConfig {
                remote_path,
                local_path,
                strm_download_url,
            },
        )
    }

    pub(crate) fn from_parts(clients: ImportClients, paths: ImportPathConfig) -> Self {
        Self { clients, paths }
    }
}
