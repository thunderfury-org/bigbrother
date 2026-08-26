use std::sync::Arc;

use crate::{
    application::{
        import_local_store::ImportLocalStore,
        ports::{
            LibraryGateway, LibraryGatewayHandle, LibraryMediaUpdate, LibraryMediaUpdateKind,
            LibraryUpdateNotifierHandle, notify_library_updates,
        },
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
    pub(super) local: ImportLocalStore,
    pub(super) metadata_lookup: MetadataLookup,
    pub(super) notifier: LibraryUpdateNotifierHandle,
}

impl TransferWorkflow {
    pub(crate) fn new(
        library_gateway: impl LibraryGateway + 'static,
        local: ImportLocalStore,
        notifier: LibraryUpdateNotifierHandle,
    ) -> Self {
        Self {
            library_gateway: Arc::new(library_gateway),
            local,
            metadata_lookup: MetadataLookup::default(),
            notifier,
        }
    }

    pub(super) fn local(&self) -> &ImportLocalStore {
        &self.local
    }

    pub(super) fn library_gateway(&self) -> &dyn LibraryGateway {
        self.library_gateway.as_ref()
    }

    pub(super) async fn queue_library_update(&self, path: String, kind: LibraryMediaUpdateKind) {
        notify_library_updates(self.notifier.as_ref(), &[LibraryMediaUpdate { path, kind }]).await;
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
