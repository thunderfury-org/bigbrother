use std::collections::HashMap;

use crate::error::AppResult;

use super::{Importer, Media, MediaFile};

impl Importer {
    pub(super) async fn group_media_files<'a>(&mut self, files: &'a [MediaFile]) -> AppResult<Vec<Media<'a>>> {
        // group files by tmdb_id
        let mut grouped_files = HashMap::new();
        for file in files {
            if file.metadata.episode_number.is_some() {
                // tv
                self.group_tv_file(file, &mut grouped_files).await?;
            } else {
                // movie
                self.group_movie_file(file, &mut grouped_files).await?;
            }
        }
        Ok(grouped_files.into_values().collect())
    }

    async fn group_tv_file<'a>(
        &mut self,
        file: &'a MediaFile,
        grouped_files: &mut HashMap<u32, Media<'a>>,
    ) -> AppResult<()> {
        let tv_info = self
            .get_tv_info_from_tmdb(&file.metadata.titles, &file.metadata.year)
            .await?;
        match tv_info {
            Some(tv_info) => {
                let season_number = match file.metadata.season_number {
                    Some(s) => s,
                    None => {
                        if tv_info.number_of_seasons == 1 {
                            1
                        } else {
                            // multi season, but no season number found in file metadata
                            self.summary.skipped += 1;
                            self.summary.unknown_files.push(file.raw.path.to_owned());
                            return Ok(());
                        }
                    }
                };
                let episode_number = match file.metadata.episode_number {
                    Some(e) => e,
                    None => {
                        // episode number not found in file metadata
                        self.summary.skipped += 1;
                        self.summary.unknown_files.push(file.raw.path.to_owned());
                        return Ok(());
                    }
                };
                let entry = grouped_files.entry(tv_info.id).or_insert_with(|| Media::Tv {
                    detail: tv_info,
                    files: HashMap::new(),
                });
                match entry {
                    Media::Tv { files, .. } => {
                        files
                            .entry(season_number)
                            .or_insert_with(HashMap::new)
                            .entry(episode_number)
                            .or_insert_with(Vec::new)
                            .push(file);
                    }
                    _ => {}
                }
            }
            None => {
                self.summary.skipped += 1;
                self.summary.unknown_files.push(file.raw.path.to_owned());
            }
        }

        Ok(())
    }

    async fn group_movie_file<'a>(
        &mut self,
        file: &'a MediaFile,
        grouped_files: &mut HashMap<u32, Media<'a>>,
    ) -> AppResult<()> {
        let movie_info = self
            .get_movie_info_from_tmdb(&file.metadata.titles, &file.metadata.year)
            .await?;
        match movie_info {
            Some(movie_info) => {
                let entry = grouped_files.entry(movie_info.id).or_insert_with(|| Media::Movie {
                    detail: movie_info,
                    files: Vec::new(),
                });
                match entry {
                    Media::Movie { files, .. } => {
                        files.push(file);
                    }
                    _ => {}
                }
            }
            None => {
                self.summary.skipped += 1;
                self.summary.unknown_files.push(file.raw.path.to_owned());
            }
        }

        Ok(())
    }
}
