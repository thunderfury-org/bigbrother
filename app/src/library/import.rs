use std::collections::HashMap;

use crate::domain::media::Metadata;

mod group;
mod inner;
pub(super) mod json;
mod library;
mod local;
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
pub(crate) use remote::{LibraryGateway, MetadataCatalog, ShareSource};

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

pub(crate) struct Importer<L, S, M> {
    library_remote: remote::LibraryRemote<L>,
    share_remote: remote::ShareRemote<S>,
    local: local::ImportLocalStore,
    tmdb_lookup: tmdb_info::TmdbLookup<M>,
    metadata_cache: HashMap<String, Box<Metadata>>,
}

impl<L, S, M> Importer<L, S, M>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
{
    pub(crate) fn new(
        library_gateway: L,
        share_source: S,
        metadata_catalog: M,
        paths: ImportPathConfig,
    ) -> Self {
        Self {
            library_remote: remote::LibraryRemote::new(library_gateway, paths.clone()),
            share_remote: remote::ShareRemote::new(share_source),
            local: local::ImportLocalStore::new(paths),
            tmdb_lookup: tmdb_info::TmdbLookup::new(metadata_catalog),
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
