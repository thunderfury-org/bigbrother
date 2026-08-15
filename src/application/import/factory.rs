use crate::{
    application::ports::{ImportLocalStore, LibraryGateway},
    domain::import::inner::Media,
    error::AppResult,
};

use super::ImportedMedia;
use super::identify::UnmatchedFile;
use super::metadata::MetadataLookup;

pub trait MediaImporter: Send + 'static {
    fn import_groups(
        &mut self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> impl std::future::Future<Output = AppResult<Vec<ImportedMedia>>> + Send;
}

#[derive(Clone)]
pub(crate) struct TransferWorkflow<L, F> {
    pub(super) library_gateway: L,
    pub(super) local: F,
    pub(super) metadata_lookup: MetadataLookup,
}

impl<L, F> TransferWorkflow<L, F> {
    pub(crate) fn new(library_gateway: L, local: F) -> Self {
        Self {
            library_gateway,
            local,
            metadata_lookup: MetadataLookup::default(),
        }
    }

    pub(super) fn local(&self) -> &F {
        &self.local
    }

    pub(super) fn library_gateway(&self) -> &L {
        &self.library_gateway
    }
}

impl<L, F> MediaImporter for TransferWorkflow<L, F>
where
    L: LibraryGateway + Send + Sync + 'static,
    F: ImportLocalStore + Send + Sync + 'static,
{
    async fn import_groups(
        &mut self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        TransferWorkflow::import_groups(self, groups, unmatched).await
    }
}
