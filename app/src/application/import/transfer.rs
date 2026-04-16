mod episode;
mod movie;
mod season;
mod tv;

use tracing::info;

use crate::domain::import::inner::{Media, MediaFile};

use super::{ImportedMedia, TransferImportUseCase, TvDetail};
use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog};
use crate::error::AppResult;

impl<L, M, F> TransferImportUseCase<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(super) async fn transfer_media_files(
        &mut self,
        media_files: &[MediaFile],
    ) -> AppResult<Vec<ImportedMedia>> {
        let medias = self.build_import_plan(media_files).await?;
        self.execute_import_plan(&medias).await
    }

    async fn build_import_plan<'a>(
        &mut self,
        media_files: &'a [MediaFile],
    ) -> AppResult<Vec<Media<'a>>> {
        let medias = self.workflow_mut().group_media_files(media_files).await?;
        info!("Grouped into {} media items", medias.len());
        Ok(medias)
    }

    async fn execute_import_plan(&mut self, medias: &[Media<'_>]) -> AppResult<Vec<ImportedMedia>> {
        let mut results = Vec::with_capacity(medias.len());
        for media in medias {
            match media {
                Media::Movie { detail, files } => {
                    if let Some(imported) = self.transfer_movie(detail, files).await? {
                        results.push(imported);
                    }
                }
                Media::Tv { detail, files } => {
                    results.extend(self.transfer_tv(detail, files).await?);
                }
            }
        }

        Ok(results)
    }
}
