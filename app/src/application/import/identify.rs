use std::collections::HashMap;

use crate::application::import_ports::{MetadataCatalog, TitleExtractor};
use crate::domain::import::inner::{Media, MediaFile};
use crate::domain::import::policy::{insert_movie_media, insert_tv_media, resolve_tv_episode_slot};
use crate::error::AppResult;

use super::tmdb_info::TmdbLookup;

#[derive(Clone)]
pub(crate) struct MediaIdentifyService<M, T> {
    tmdb_lookup: TmdbLookup<M, T>,
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

impl<M, T> MediaIdentifyService<M, T>
where
    M: MetadataCatalog,
    T: TitleExtractor,
{
    pub fn new(metadata_catalog: M, title_extractor: T) -> Self {
        Self {
            tmdb_lookup: TmdbLookup::new(metadata_catalog, title_extractor),
        }
    }

    pub async fn identify(&mut self, files: &[MediaFile]) -> AppResult<IdentifyOutcome> {
        let mut grouped: HashMap<u32, Media> = HashMap::new();
        let mut unmatched = Vec::new();

        for file in files {
            if file.metadata.is_tv_episode() {
                if self.identify_tv(file, &mut grouped).await?.is_some() {
                    unmatched.push(UnmatchedFile {
                        file_name: file.video.name.clone(),
                        file_path: file.video.path.clone(),
                    });
                }
            } else if self.identify_movie(file, &mut grouped).await?.is_some() {
                unmatched.push(UnmatchedFile {
                    file_name: file.video.name.clone(),
                    file_path: file.video.path.clone(),
                });
            }
        }

        Ok(IdentifyOutcome {
            groups: grouped.into_values().collect(),
            unmatched,
        })
    }

    async fn identify_tv(
        &mut self,
        file: &MediaFile,
        grouped: &mut HashMap<u32, Media>,
    ) -> AppResult<Option<()>> {
        let tv_info = self
            .tmdb_lookup
            .get_tv_info(&file.metadata, &file.descriptions)
            .await?;
        match tv_info {
            Some(tv_info) => {
                let Some((season_number, episode_number)) = resolve_tv_episode_slot(file, &tv_info)
                else {
                    return Ok(Some(()));
                };
                insert_tv_media(
                    grouped,
                    tv_info,
                    season_number,
                    episode_number,
                    file.clone(),
                );
                Ok(None)
            }
            None => Ok(Some(())),
        }
    }

    async fn identify_movie(
        &mut self,
        file: &MediaFile,
        grouped: &mut HashMap<u32, Media>,
    ) -> AppResult<Option<()>> {
        let movie_info = self
            .tmdb_lookup
            .get_movie_info(&file.metadata, &file.descriptions)
            .await?;
        match movie_info {
            Some(movie_info) => {
                insert_movie_media(grouped, movie_info, file.clone());
                Ok(None)
            }
            None => Ok(Some(())),
        }
    }
}

#[cfg(test)]
#[path = "identify/tests.rs"]
mod tests;
