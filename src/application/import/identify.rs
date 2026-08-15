use std::collections::HashMap;

use crate::application::ports::{MetadataCatalog, TitleExtractor};
use crate::domain::import::inner::{Media, MediaFile};
use crate::domain::import::policy::{insert_movie_media, insert_tv_media, resolve_tv_episode_slot};
use crate::error::AppResult;
use std::sync::Arc;

use super::tmdb_info::TmdbLookup;

#[derive(Clone)]
pub(crate) struct MediaIdentifyService {
    tmdb_lookup: TmdbLookup,
}

pub(crate) struct IdentifyOutcome {
    pub groups: Vec<Media>,
    pub unmatched: Vec<UnmatchedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnmatchedFile {
    pub file_name: String,
    pub file_path: String,
}

impl From<MediaFile> for UnmatchedFile {
    fn from(file: MediaFile) -> Self {
        Self {
            file_name: file.video.name,
            file_path: file.video.path,
        }
    }
}

#[async_trait::async_trait]
pub(crate) trait MediaIdentifier: Send + Sync {
    async fn identify(&self, files: Vec<MediaFile>) -> AppResult<IdentifyOutcome>;
}

pub(crate) type MediaIdentifierHandle = Arc<dyn MediaIdentifier>;

impl MediaIdentifyService {
    pub fn new(
        metadata_catalog: impl MetadataCatalog + 'static,
        title_extractor: impl TitleExtractor + 'static,
    ) -> Self {
        Self {
            tmdb_lookup: TmdbLookup::new(metadata_catalog, title_extractor),
        }
    }

    pub async fn identify(&self, files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
        let mut grouped: HashMap<u32, Media> = HashMap::new();
        let mut unmatched = Vec::new();

        for file in files {
            let unmatched_file = if file.metadata.is_tv_episode() {
                self.identify_tv(file, &mut grouped).await?
            } else {
                self.identify_movie(file, &mut grouped).await?
            };
            if let Some(file) = unmatched_file {
                unmatched.push(file.into());
            }
        }

        Ok(IdentifyOutcome {
            groups: grouped.into_values().collect(),
            unmatched,
        })
    }

    async fn identify_tv(
        &self,
        file: MediaFile,
        grouped: &mut HashMap<u32, Media>,
    ) -> AppResult<Option<MediaFile>> {
        let tv_info = self
            .tmdb_lookup
            .get_tv_info(&file.metadata, &file.descriptions)
            .await?;
        match tv_info {
            Some(tv_info) => {
                let Some((season_number, episode_number)) =
                    resolve_tv_episode_slot(&file, &tv_info)
                else {
                    return Ok(Some(file));
                };
                insert_tv_media(grouped, tv_info, season_number, episode_number, file);
                Ok(None)
            }
            None => Ok(Some(file)),
        }
    }

    async fn identify_movie(
        &self,
        file: MediaFile,
        grouped: &mut HashMap<u32, Media>,
    ) -> AppResult<Option<MediaFile>> {
        let movie_info = self
            .tmdb_lookup
            .get_movie_info(&file.metadata, &file.descriptions)
            .await?;
        match movie_info {
            Some(movie_info) => {
                insert_movie_media(grouped, movie_info, file);
                Ok(None)
            }
            None => Ok(Some(file)),
        }
    }
}

#[async_trait::async_trait]
impl MediaIdentifier for MediaIdentifyService {
    async fn identify(&self, files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
        MediaIdentifyService::identify(self, files).await
    }
}

#[cfg(test)]
#[path = "identify/tests.rs"]
mod tests;
