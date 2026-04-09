use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};
use std::ops::{Deref, DerefMut};

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

#[derive(Clone)]
pub(crate) struct ImportContext<L, S, M, F> {
    library_gateway: L,
    share_source: S,
    metadata_catalog: M,
    local: F,
}

pub(crate) struct ImportWorkflow<L, S, M, F> {
    library_gateway: L,
    share_source: S,
    metadata_catalog: M,
    local: F,
    tmdb_lookup: tmdb_info::TmdbLookup<M>,
    metadata_lookup: metadata::MetadataLookup,
}

pub(crate) struct ShareImportUseCase<L, S, M, F> {
    workflow: ImportWorkflow<L, S, M, F>,
}

pub(crate) struct JsonImportUseCase<L, S, M, F> {
    workflow: ImportWorkflow<L, S, M, F>,
}

pub(crate) struct TransferImportUseCase<L, S, M, F> {
    workflow: ImportWorkflow<L, S, M, F>,
}

impl<L, S, M, F> ImportContext<L, S, M, F>
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
            metadata_catalog,
            local: local_store,
        }
    }

    fn workflow(&self) -> ImportWorkflow<L, S, M, F> {
        ImportWorkflow {
            library_gateway: self.library_gateway.clone(),
            share_source: self.share_source.clone(),
            metadata_catalog: self.metadata_catalog.clone(),
            local: self.local.clone(),
            tmdb_lookup: tmdb_info::TmdbLookup::new(self.metadata_catalog.clone()),
            metadata_lookup: metadata::MetadataLookup::default(),
        }
    }
}

impl<L, S, M, F> ShareImportUseCase<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(crate) fn new(context: ImportContext<L, S, M, F>) -> Self {
        Self {
            workflow: context.workflow(),
        }
    }
}

impl<L, S, M, F> JsonImportUseCase<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(crate) fn new(context: ImportContext<L, S, M, F>) -> Self {
        Self {
            workflow: context.workflow(),
        }
    }
}

impl<L, S, M, F> TransferImportUseCase<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(crate) fn new(context: ImportContext<L, S, M, F>) -> Self {
        Self {
            workflow: context.workflow(),
        }
    }
}

impl<L, S, M, F> ImportWorkflow<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    fn context(&self) -> ImportContext<L, S, M, F> {
        ImportContext::new(
            self.library_gateway.clone(),
            self.share_source.clone(),
            self.metadata_catalog.clone(),
            self.local.clone(),
        )
    }
}

impl<L, S, M, F> Deref for ShareImportUseCase<L, S, M, F> {
    type Target = ImportWorkflow<L, S, M, F>;

    fn deref(&self) -> &Self::Target {
        &self.workflow
    }
}

impl<L, S, M, F> DerefMut for ShareImportUseCase<L, S, M, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.workflow
    }
}

impl<L, S, M, F> Deref for JsonImportUseCase<L, S, M, F> {
    type Target = ImportWorkflow<L, S, M, F>;

    fn deref(&self) -> &Self::Target {
        &self.workflow
    }
}

impl<L, S, M, F> DerefMut for JsonImportUseCase<L, S, M, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.workflow
    }
}

impl<L, S, M, F> Deref for TransferImportUseCase<L, S, M, F> {
    type Target = ImportWorkflow<L, S, M, F>;

    fn deref(&self) -> &Self::Target {
        &self.workflow
    }
}

impl<L, S, M, F> DerefMut for TransferImportUseCase<L, S, M, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.workflow
    }
}
