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

#[derive(Clone)]
pub(crate) struct ImportUseCaseFactory<L, S, M, F> {
    library_gateway: L,
    share_source: S,
    metadata_catalog: M,
    local: F,
}

pub(crate) struct TransferWorkflow<L, M, F> {
    library_gateway: L,
    local: F,
    tmdb_lookup: tmdb_info::TmdbLookup<M>,
    metadata_lookup: metadata::MetadataLookup,
}

pub(crate) struct ShareImportUseCase<L, S, M, F> {
    share_source: S,
    metadata_lookup: metadata::MetadataLookup,
    transfer: TransferImportUseCase<L, M, F>,
}

pub(crate) struct JsonImportUseCase<L, M, F> {
    metadata_lookup: metadata::MetadataLookup,
    transfer: TransferImportUseCase<L, M, F>,
}

pub(crate) struct TransferImportUseCase<L, M, F> {
    workflow: TransferWorkflow<L, M, F>,
}

impl<L, S, M, F> ImportUseCaseFactory<L, S, M, F> {
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
}

impl<L, S, M, F> ImportUseCaseFactory<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    fn transfer_workflow(&self) -> TransferWorkflow<L, M, F> {
        TransferWorkflow {
            library_gateway: self.library_gateway.clone(),
            local: self.local.clone(),
            tmdb_lookup: tmdb_info::TmdbLookup::new(self.metadata_catalog.clone()),
            metadata_lookup: metadata::MetadataLookup::default(),
        }
    }

    pub(crate) fn share_import(&self) -> ShareImportUseCase<L, S, M, F> {
        ShareImportUseCase::new(self.share_source.clone(), self.transfer_workflow())
    }

    pub(crate) fn json_import(&self) -> JsonImportUseCase<L, M, F> {
        JsonImportUseCase::new(self.transfer_workflow())
    }
}

impl<L, S, M, F> ShareImportUseCase<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    fn new(share_source: S, transfer_workflow: TransferWorkflow<L, M, F>) -> Self {
        Self {
            share_source,
            metadata_lookup: metadata::MetadataLookup::default(),
            transfer: TransferImportUseCase::new(transfer_workflow),
        }
    }

    fn share_source(&self) -> &S {
        &self.share_source
    }

    fn metadata_lookup_mut(&mut self) -> &mut metadata::MetadataLookup {
        &mut self.metadata_lookup
    }

    fn transfer_mut(&mut self) -> &mut TransferImportUseCase<L, M, F> {
        &mut self.transfer
    }
}

impl<L, M, F> JsonImportUseCase<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    fn new(transfer_workflow: TransferWorkflow<L, M, F>) -> Self {
        Self {
            metadata_lookup: metadata::MetadataLookup::default(),
            transfer: TransferImportUseCase::new(transfer_workflow),
        }
    }

    fn metadata_lookup_mut(&mut self) -> &mut metadata::MetadataLookup {
        &mut self.metadata_lookup
    }

    fn transfer_mut(&mut self) -> &mut TransferImportUseCase<L, M, F> {
        &mut self.transfer
    }
}

impl<L, M, F> TransferImportUseCase<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(crate) fn new(workflow: TransferWorkflow<L, M, F>) -> Self {
        Self { workflow }
    }

    fn workflow(&self) -> &TransferWorkflow<L, M, F> {
        &self.workflow
    }

    fn workflow_mut(&mut self) -> &mut TransferWorkflow<L, M, F> {
        &mut self.workflow
    }
}

impl<L, M, F> TransferWorkflow<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    fn local(&self) -> &F {
        &self.local
    }

    fn library_gateway(&self) -> &L {
        &self.library_gateway
    }
}
