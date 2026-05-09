use super::{metadata, tmdb_info};
use crate::application::import_ports::MetadataCatalog;

#[derive(Clone)]
pub(crate) struct TransferWorkflow<L, M, F> {
    pub(super) library_gateway: L,
    pub(super) local: F,
    pub(super) tmdb_lookup: tmdb_info::TmdbLookup<M>,
    pub(super) metadata_lookup: metadata::MetadataLookup,
}

impl<L, M, F> TransferWorkflow<L, M, F> {
    pub(crate) fn new(library_gateway: L, metadata_catalog: M, local: F) -> Self
    where
        M: MetadataCatalog,
    {
        Self {
            library_gateway,
            local,
            tmdb_lookup: tmdb_info::TmdbLookup::new(metadata_catalog),
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
