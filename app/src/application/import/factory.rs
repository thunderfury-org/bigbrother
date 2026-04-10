use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};

use super::{metadata, tmdb_info};

#[derive(Clone)]
pub(crate) struct ImportUseCaseFactory<L, S, M, F> {
    library_gateway: L,
    share_source: S,
    metadata_catalog: M,
    local: F,
}

pub(crate) struct TransferWorkflow<L, M, F> {
    pub(super) library_gateway: L,
    pub(super) local: F,
    pub(super) tmdb_lookup: tmdb_info::TmdbLookup<M>,
    pub(super) metadata_lookup: metadata::MetadataLookup,
}

pub(crate) struct ShareImportUseCase<L, S, M, F> {
    pub(super) share_source: S,
    pub(super) metadata_lookup: metadata::MetadataLookup,
    pub(super) transfer: TransferImportUseCase<L, M, F>,
}

pub(crate) struct JsonImportUseCase<L, M, F> {
    pub(super) metadata_lookup: metadata::MetadataLookup,
    pub(super) transfer: TransferImportUseCase<L, M, F>,
}

pub(crate) struct TransferImportUseCase<L, M, F> {
    pub(super) workflow: TransferWorkflow<L, M, F>,
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

    pub(super) fn share_source(&self) -> &S {
        &self.share_source
    }

    pub(super) fn metadata_lookup_mut(&mut self) -> &mut metadata::MetadataLookup {
        &mut self.metadata_lookup
    }

    pub(super) fn transfer_mut(&mut self) -> &mut TransferImportUseCase<L, M, F> {
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

    pub(super) fn metadata_lookup_mut(&mut self) -> &mut metadata::MetadataLookup {
        &mut self.metadata_lookup
    }

    pub(super) fn transfer_mut(&mut self) -> &mut TransferImportUseCase<L, M, F> {
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

    pub(super) fn workflow(&self) -> &TransferWorkflow<L, M, F> {
        &self.workflow
    }

    pub(super) fn workflow_mut(&mut self) -> &mut TransferWorkflow<L, M, F> {
        &mut self.workflow
    }
}

impl<L, M, F> TransferWorkflow<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(super) fn local(&self) -> &F {
        &self.local
    }

    pub(super) fn library_gateway(&self) -> &L {
        &self.library_gateway
    }
}
