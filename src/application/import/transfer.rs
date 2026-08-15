mod episode;
mod movie;
mod season;
mod tv;

use std::collections::BTreeMap;

use tracing::info;

use crate::domain::import::inner::{Media, MediaFile};

use super::identify::UnmatchedFile;
use super::{ImportedMedia, TransferWorkflow};
use crate::application::import_ports::{ImportLocalStore, LibraryGateway};
use crate::error::AppResult;

impl<L, F> TransferWorkflow<L, F>
where
    L: LibraryGateway,
    F: ImportLocalStore,
{
    pub(crate) async fn import_groups(
        &mut self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        let mut results = self.execute_import_plan(&groups).await?;
        info!(
            media_group_count = groups.len(),
            unmatched_count = unmatched.len(),
            "Executed import plan"
        );

        if !unmatched.is_empty() {
            results.push(ImportedMedia::Skipped {
                count: unmatched.len(),
                files: unmatched
                    .iter()
                    .map(|u| format!("{}/{}", u.file_path, u.file_name))
                    .collect(),
            });
        }

        Ok(results)
    }

    async fn execute_import_plan(&mut self, medias: &[Media]) -> AppResult<Vec<ImportedMedia>> {
        let mut results = Vec::with_capacity(medias.len());
        for media in medias {
            match media {
                Media::Movie { detail, files } => {
                    let refs: Vec<&MediaFile> = files.iter().collect();
                    if let Some(imported) = self.transfer_movie(detail, &refs).await? {
                        results.push(imported);
                    }
                }
                Media::Tv { detail, files } => {
                    let refs = borrow_tv_files(files);
                    results.extend(self.transfer_tv(detail, &refs).await?);
                }
            }
        }

        Ok(results)
    }
}

fn borrow_tv_files(
    files: &BTreeMap<u32, BTreeMap<u32, Vec<MediaFile>>>,
) -> BTreeMap<u32, BTreeMap<u32, Vec<&MediaFile>>> {
    files
        .iter()
        .map(|(&season, episodes)| {
            let eps = episodes
                .iter()
                .map(|(&ep, files)| (ep, files.iter().collect()))
                .collect();
            (season, eps)
        })
        .collect()
}
