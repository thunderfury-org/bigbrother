mod episode;
mod movie;
mod season;
mod tv;

use tracing::info;

use crate::domain::import::inner::{Media, MediaFile};

use super::{ImportedMedia, TransferWorkflow, TvDetail};
use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, TitleExtractor,
};
use crate::error::AppResult;

impl<L, M, F, T> TransferWorkflow<L, M, F, T>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
    T: TitleExtractor,
{
    pub(crate) async fn transfer_media_files(
        &mut self,
        media_files: &[MediaFile],
    ) -> AppResult<Vec<ImportedMedia>> {
        let (medias, unmatched) = self.build_import_plan(media_files).await?;
        let mut results = self.execute_import_plan(&medias).await?;
        info!(
            media_group_count = medias.len(),
            unmatched_count = unmatched.len(),
            "Executed import plan"
        );

        if !unmatched.is_empty() {
            results.push(ImportedMedia::Skipped {
                count: unmatched.len(),
                files: unmatched
                    .iter()
                    .map(|(name, path)| format!("{path}/{name}"))
                    .collect(),
            });
        }

        Ok(results)
    }

    async fn build_import_plan<'a>(
        &mut self,
        media_files: &'a [MediaFile],
    ) -> AppResult<(Vec<Media<'a>>, Vec<(&'a str, &'a str)>)> {
        let (medias, unmatched) = self.group_media_files(media_files).await?;
        info!(
            "Grouped into {} media items, {} unmatched",
            medias.len(),
            unmatched.len()
        );
        Ok((medias, unmatched))
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
