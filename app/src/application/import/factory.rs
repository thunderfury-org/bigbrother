use super::{metadata, tmdb_info};
use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MediaImporter, MetadataCatalog, TitleExtractor,
};

#[derive(Clone)]
pub(crate) struct TransferWorkflow<L, M, F, T> {
    pub(super) library_gateway: L,
    pub(super) local: F,
    pub(super) tmdb_lookup: tmdb_info::TmdbLookup<M, T>,
    pub(super) metadata_lookup: metadata::MetadataLookup,
}

impl<L, M, F, T> TransferWorkflow<L, M, F, T> {
    pub(crate) fn new(library_gateway: L, metadata_catalog: M, local: F, title_extractor: T) -> Self
    where
        M: MetadataCatalog,
        T: TitleExtractor,
    {
        Self {
            library_gateway,
            local,
            tmdb_lookup: tmdb_info::TmdbLookup::new(metadata_catalog, title_extractor),
            metadata_lookup: metadata::MetadataLookup::default(),
        }
    }

    pub(super) fn local(&self) -> &F {
        &self.local
    }

    pub(super) fn library_gateway(&self) -> &L {
        &self.library_gateway
    }
}

impl<L, M, F, T> MediaImporter for TransferWorkflow<L, M, F, T>
where
    L: LibraryGateway + Send + Sync + 'static,
    M: MetadataCatalog + Send + Sync + 'static,
    F: ImportLocalStore + Send + Sync + 'static,
    T: TitleExtractor + Send + Sync + 'static,
{
    async fn transfer_media_files(
        &mut self,
        media_files: &[crate::domain::import::inner::MediaFile],
    ) -> crate::error::AppResult<Vec<super::ImportedMedia>> {
        self.transfer_media_files(media_files).await
    }
}
