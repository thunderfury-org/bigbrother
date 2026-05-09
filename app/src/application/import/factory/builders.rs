use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};

#[cfg(test)]
use super::JsonImportUseCase;
use super::{
    ImportUseCaseFactory, ShareImportUseCase, TransferImportUseCase, TransferWorkflow, metadata,
    tmdb_info,
};

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
        ShareImportUseCase::new(self.transfer_workflow())
    }

    #[cfg(test)]
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
    fn new(transfer_workflow: TransferWorkflow<L, M, F>) -> Self {
        Self {
            metadata_lookup: metadata::MetadataLookup::default(),
            transfer: TransferImportUseCase::new(transfer_workflow),
            _phantom: std::marker::PhantomData,
        }
    }

    pub(in crate::application::import) fn metadata_lookup_mut(
        &mut self,
    ) -> &mut metadata::MetadataLookup {
        &mut self.metadata_lookup
    }

    pub(in crate::application::import) fn transfer_mut(
        &mut self,
    ) -> &mut TransferImportUseCase<L, M, F> {
        &mut self.transfer
    }
}

#[cfg(test)]
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

    pub(in crate::application::import) fn metadata_lookup_mut(
        &mut self,
    ) -> &mut metadata::MetadataLookup {
        &mut self.metadata_lookup
    }

    pub(in crate::application::import) fn transfer_mut(
        &mut self,
    ) -> &mut TransferImportUseCase<L, M, F> {
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

    pub(in crate::application::import) fn workflow(&self) -> &TransferWorkflow<L, M, F> {
        &self.workflow
    }

    pub(in crate::application::import) fn workflow_mut(
        &mut self,
    ) -> &mut TransferWorkflow<L, M, F> {
        &mut self.workflow
    }
}

impl<L, M, F> TransferWorkflow<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(in crate::application::import) fn local(&self) -> &F {
        &self.local
    }

    pub(in crate::application::import) fn library_gateway(&self) -> &L {
        &self.library_gateway
    }
}
