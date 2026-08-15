use std::sync::Arc;

use crate::{
    application::ports::{
        ImportLocalStore, ImportLocalStoreHandle, LibraryGateway, LibraryGatewayHandle,
        erase::{DynImportLocalStore, DynLibraryGateway},
    },
    domain::import::inner::Media,
    error::AppResult,
};

use super::ImportedMedia;
use super::identify::UnmatchedFile;
use super::metadata::MetadataLookup;
use futures::future::BoxFuture;

pub trait MediaImporter: Send + Sync + 'static {
    fn import_groups(
        &self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> impl std::future::Future<Output = AppResult<Vec<ImportedMedia>>> + Send;
}

pub trait DynMediaImporter: Send + Sync {
    fn import_groups(
        &self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> BoxFuture<'_, AppResult<Vec<ImportedMedia>>>;
}

impl<T: MediaImporter> DynMediaImporter for T {
    fn import_groups(
        &self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> BoxFuture<'_, AppResult<Vec<ImportedMedia>>> {
        Box::pin(MediaImporter::import_groups(self, groups, unmatched))
    }
}

pub type MediaImporterHandle = Arc<dyn DynMediaImporter>;

#[derive(Clone)]
pub(crate) struct TransferWorkflow {
    pub(super) library_gateway: LibraryGatewayHandle,
    pub(super) local: ImportLocalStoreHandle,
    pub(super) metadata_lookup: MetadataLookup,
}

impl TransferWorkflow {
    pub(crate) fn new(
        library_gateway: impl LibraryGateway + Send + Sync + 'static,
        local: impl ImportLocalStore + Send + Sync + 'static,
    ) -> Self {
        Self {
            library_gateway: Arc::new(library_gateway),
            local: Arc::new(local),
            metadata_lookup: MetadataLookup::default(),
        }
    }

    pub(super) fn local(&self) -> &dyn DynImportLocalStore {
        self.local.as_ref()
    }

    pub(super) fn library_gateway(&self) -> &dyn DynLibraryGateway {
        self.library_gateway.as_ref()
    }
}

impl MediaImporter for TransferWorkflow {
    async fn import_groups(
        &self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        TransferWorkflow::import_groups(self, groups, unmatched).await
    }
}
