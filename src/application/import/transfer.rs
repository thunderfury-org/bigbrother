mod episode;
mod movie;
mod season;
mod tv;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tracing::info;

use crate::application::ports::{
    LibraryMediaUpdate, LibraryUpdateNotifier, notify_library_updates,
};
use crate::domain::import::inner::{Media, MediaFile};
use crate::error::AppResult;

use super::identify::UnmatchedFile;
use super::{ImportedMedia, TransferWorkflow};

impl TransferWorkflow {
    pub(crate) async fn import_groups(
        &self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        let buffer = BufferingLibraryUpdateNotifier::default();
        let scoped = Self {
            library_gateway: self.library_gateway.clone(),
            local: self.local.clone(),
            metadata_lookup: self.metadata_lookup.clone(),
            notifier: Arc::new(buffer.clone()),
        };
        let result = scoped.execute_import_plan(&groups).await;
        notify_library_updates(self.notifier.as_ref(), &buffer.take()).await;
        let mut results = result?;
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

    async fn execute_import_plan(&self, medias: &[Media]) -> AppResult<Vec<ImportedMedia>> {
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

#[derive(Clone, Default)]
struct BufferingLibraryUpdateNotifier {
    updates: Arc<Mutex<Vec<LibraryMediaUpdate>>>,
}

impl BufferingLibraryUpdateNotifier {
    fn take(&self) -> Vec<LibraryMediaUpdate> {
        std::mem::take(&mut *self.updates.lock().unwrap())
    }
}

#[async_trait::async_trait]
impl LibraryUpdateNotifier for BufferingLibraryUpdateNotifier {
    async fn notify(&self, updates: &[LibraryMediaUpdate]) -> AppResult<()> {
        self.updates.lock().unwrap().extend_from_slice(updates);
        Ok(())
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
