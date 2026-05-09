mod builders;

use super::{metadata, tmdb_info};

#[derive(Clone)]
pub(crate) struct ImportUseCaseFactory<L, S, M, F> {
    library_gateway: L,
    metadata_catalog: M,
    local: F,
    _phantom: std::marker::PhantomData<S>,
}

pub(crate) struct TransferWorkflow<L, M, F> {
    pub(super) library_gateway: L,
    pub(super) local: F,
    pub(super) tmdb_lookup: tmdb_info::TmdbLookup<M>,
    pub(super) metadata_lookup: metadata::MetadataLookup,
}

pub(crate) struct ShareImportUseCase<L, S, M, F> {
    pub(super) metadata_lookup: metadata::MetadataLookup,
    pub(super) transfer: TransferImportUseCase<L, M, F>,
    _phantom: std::marker::PhantomData<S>,
}

#[allow(dead_code)]
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
        _share_source: S,
        metadata_catalog: M,
        local_store: F,
    ) -> Self {
        Self {
            library_gateway,
            metadata_catalog,
            local: local_store,
            _phantom: std::marker::PhantomData,
        }
    }
}
