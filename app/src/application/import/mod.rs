use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};

mod group;
pub(super) mod json;
mod library;
mod metadata;
pub(super) mod share;
mod tmdb_info;
mod transfer;
mod transfer_cleanup;
mod transfer_save;
mod transfer_support;
mod transfer_target;

pub(crate) use crate::domain::import::{
    Genre, LibraryFile, MovieDetail, Pan115FileEntry, Pan189File, Pan189Folder, Pan189ShareInfo,
    SearchMovieResult, SearchTvResult, Season, TvDetail,
};
pub(crate) use crate::domain::import::{ShareUrl, is_fslink};

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

pub(crate) struct Importer<L, S, M, F> {
    library_gateway: L,
    share_source: S,
    local: F,
    tmdb_lookup: tmdb_info::TmdbLookup<M>,
    metadata_lookup: metadata::MetadataLookup,
}

impl<L, S, M, F> Importer<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(crate) fn new(
        library_gateway: L,
        share_source: S,
        metadata_catalog: M,
        local_store: F,
    ) -> Self {
        Self {
            library_gateway,
            share_source,
            local: local_store,
            tmdb_lookup: tmdb_info::TmdbLookup::new(metadata_catalog),
            metadata_lookup: metadata::MetadataLookup::default(),
        }
    }
}
