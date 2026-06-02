use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MediaImporter};

use super::ImportedMedia;
use super::identify::UnmatchedFile;
use super::metadata::MetadataLookup;

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
        groups: Vec<crate::domain::import::inner::Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> crate::error::AppResult<Vec<ImportedMedia>> {
        self.import_groups(groups, unmatched).await
    }
}
