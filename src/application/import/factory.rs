use std::sync::Arc;

use crate::{
    application::ports::{
        ImportLocalStore, ImportLocalStoreHandle, LibraryGateway, LibraryGatewayHandle,
    },
    domain::import::inner::Media,
    error::AppResult,
};

use super::ImportedMedia;
use super::identify::UnmatchedFile;
use super::metadata::MetadataLookup;

#[async_trait::async_trait]
pub trait MediaImporter: Send + Sync {
    async fn import_groups(
        &self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> AppResult<Vec<ImportedMedia>>;
}

pub type MediaImporterHandle = Arc<dyn MediaImporter>;

#[derive(Clone)]
pub(crate) struct TransferWorkflow {
    pub(super) library_gateway: LibraryGatewayHandle,
    pub(super) local: ImportLocalStoreHandle,
    pub(super) metadata_lookup: MetadataLookup,
}

impl TransferWorkflow {
    pub(crate) fn new(
        library_gateway: impl LibraryGateway + 'static,
        local: impl ImportLocalStore + 'static,
    ) -> Self {
        Self {
            library_gateway: Arc::new(library_gateway),
            local: Arc::new(local),
            metadata_lookup: MetadataLookup::default(),
        }
    }

    pub(super) fn local(&self) -> &dyn ImportLocalStore {
        self.local.as_ref()
    }

    pub(super) fn library_gateway(&self) -> &dyn LibraryGateway {
        self.library_gateway.as_ref()
    }
}

#[async_trait::async_trait]
impl MediaImporter for TransferWorkflow {
    async fn import_groups(
        &self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        TransferWorkflow::import_groups(self, groups, unmatched).await
    }
}
