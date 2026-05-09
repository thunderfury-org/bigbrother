use crate::{
    application::import::{ImportedMedia, MetadataLookup, TransferWorkflow},
    application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog},
    domain::import::inner::RawFile,
    error::AppResult,
};

pub async fn import_with_raw_files<L, M, F>(
    transfer: &mut TransferWorkflow<L, M, F>,
    metadata_lookup: &mut MetadataLookup,
    raw_files: Vec<RawFile>,
) -> AppResult<Vec<ImportedMedia>>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    let media_files = metadata_lookup.build_media_files(raw_files);
    transfer.transfer_media_files(&media_files).await
}

#[cfg(test)]
use crate::application::{
    import::ShareUrl, import_ports::ShareSource, share_crawler::ShareCrawler,
};

#[cfg(test)]
pub(crate) struct TestImportService<L, S, M, F> {
    pub crawler: ShareCrawler<S>,
    pub transfer: TransferWorkflow<L, M, F>,
    pub metadata_lookup: MetadataLookup,
}

#[cfg(test)]
impl<L, S, M, F> TestImportService<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub fn new(library_gateway: L, share_source: S, metadata_catalog: M, local_store: F) -> Self {
        Self {
            crawler: ShareCrawler::new(share_source),
            transfer: TransferWorkflow::new(library_gateway, metadata_catalog, local_store),
            metadata_lookup: MetadataLookup::default(),
        }
    }

    pub async fn import_from_share_url(
        &mut self,
        url: &ShareUrl<'_>,
    ) -> AppResult<Vec<ImportedMedia>> {
        let raw_files = self.crawler.raw_files_from_share_url(url).await?;
        import_with_raw_files(&mut self.transfer, &mut self.metadata_lookup, raw_files).await
    }

    pub async fn import_from_fslink(&mut self, fslink: &str) -> AppResult<Vec<ImportedMedia>> {
        let raw_files = self.crawler.raw_files_from_fslink(fslink)?;
        import_with_raw_files(&mut self.transfer, &mut self.metadata_lookup, raw_files).await
    }

    pub async fn import_from_json(&mut self, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
        let raw_files = self.crawler.raw_files_from_json(json)?;
        import_with_raw_files(&mut self.transfer, &mut self.metadata_lookup, raw_files).await
    }
}

#[cfg(test)]
#[path = "import_media/tests.rs"]
mod tests;
