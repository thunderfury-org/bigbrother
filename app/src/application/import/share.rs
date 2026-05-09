use super::{ImportedMedia, ShareImportUseCase};
use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog};
use crate::domain::import::inner::RawFile;
use crate::error::AppResult;

impl<L, S, M, F> ShareImportUseCase<L, S, M, F>
where
    L: LibraryGateway,
    S: crate::application::import_ports::ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub async fn import_from_raw_files(
        &mut self,
        raw_files: Vec<RawFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        let media_files = self.metadata_lookup_mut().build_media_files(raw_files);
        self.transfer_mut().transfer_media_files(&media_files).await
    }
}
