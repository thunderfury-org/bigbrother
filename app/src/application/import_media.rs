use crate::{
    application::import::{ImportUseCaseFactory, ImportedMedia, ShareUrl},
    application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource},
    domain::import::inner::RawFile,
    error::AppResult,
};

#[derive(Clone)]
pub struct ImportMediaService<L, S, M, F> {
    import_use_cases: ImportUseCaseFactory<L, S, M, F>,
}

impl<L, S, M, F> ImportMediaService<L, S, M, F> {
    pub fn new(library_gateway: L, share_source: S, metadata_catalog: M, local_store: F) -> Self {
        Self {
            import_use_cases: ImportUseCaseFactory::new(
                library_gateway,
                share_source,
                metadata_catalog,
                local_store,
            ),
        }
    }
}

impl<L, S, M, F> ImportMediaService<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub async fn import_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<ImportedMedia>> {
        self.import_use_cases
            .share_import()
            .import_from_share_url(url)
            .await
    }

    pub async fn import_from_fslink(&self, fslink: &str) -> AppResult<Vec<ImportedMedia>> {
        self.import_use_cases
            .json_import()
            .import_from_fslink(fslink)
            .await
    }

    pub async fn import_from_json(&self, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
        self.import_use_cases
            .json_import()
            .import_from_json(json)
            .await
    }

    pub async fn raw_files_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<RawFile>> {
        self.import_use_cases
            .share_import()
            .raw_files_from_share_url(url)
            .await
    }

    pub async fn raw_files_from_fslink(&self, fslink: &str) -> AppResult<Vec<RawFile>> {
        self.import_use_cases
            .json_import()
            .raw_files_from_fslink(fslink)
    }

    pub async fn raw_files_from_json(&self, json: Vec<u8>) -> AppResult<Vec<RawFile>> {
        self.import_use_cases
            .json_import()
            .raw_files_from_json(json)
    }
}

#[cfg(test)]
#[path = "import_media/tests.rs"]
mod tests;
