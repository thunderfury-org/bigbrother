use crate::{
    application::ports::{MetadataCatalog, TitleExtractor},
    domain::share::RawFile,
    error::AppResult,
};

use super::{MetadataLookup, tmdb_info::TmdbLookup};

pub(crate) enum ParsedMediaInfo {
    Movie {
        file_name: String,
        path: String,
        titles: Vec<String>,
        year: String,
        resolution: String,
        quality: String,
        video_codec: String,
        audio_codec: String,
        hdr: String,
        tmdb_id: Option<u32>,
        tmdb_title: Option<String>,
        tmdb_year: Option<String>,
        subtitle_files: Vec<String>,
    },
    Tv {
        file_name: String,
        path: String,
        titles: Vec<String>,
        year: String,
        season_number: Option<u32>,
        episode_number: Option<u32>,
        second_episode_number: Option<u32>,
        resolution: String,
        quality: String,
        video_codec: String,
        audio_codec: String,
        hdr: String,
        tmdb_id: Option<u32>,
        tmdb_title: Option<String>,
        tmdb_year: Option<String>,
        subtitle_files: Vec<String>,
    },
    Unmatched {
        file_name: String,
        path: String,
        titles: Vec<String>,
        year: String,
    },
}

pub(crate) struct ParseService<M, T> {
    metadata_lookup: MetadataLookup,
    tmdb_lookup: TmdbLookup<M, T>,
}

impl<M, T> ParseService<M, T>
where
    M: MetadataCatalog,
    T: TitleExtractor,
{
    pub(crate) fn new(metadata_catalog: M, title_extractor: T) -> Self {
        Self {
            metadata_lookup: MetadataLookup::default(),
            tmdb_lookup: TmdbLookup::new(metadata_catalog, title_extractor),
        }
    }

    pub(crate) async fn parse_media_files(
        &mut self,
        raw_files: Vec<RawFile>,
        descriptions: Vec<String>,
    ) -> AppResult<Vec<ParsedMediaInfo>> {
        let media_files = self
            .metadata_lookup
            .build_media_files(raw_files, descriptions);
        let mut results = Vec::new();

        for file in &media_files {
            let m = &file.metadata;
            let titles: Vec<String> = m.titles.iter().map(|t| t.title.clone()).collect();
            let path = if file.video.path.is_empty() {
                file.video.name.clone()
            } else {
                format!("{}/{}", file.video.path, file.video.name)
            };
            let subtitle_files: Vec<String> =
                file.subtitles.iter().map(|s| s.name.clone()).collect();

            if m.is_tv_episode() {
                let tv_info = self.tmdb_lookup.get_tv_info(m, &file.descriptions).await?;
                match tv_info {
                    Some(tv) => results.push(ParsedMediaInfo::Tv {
                        file_name: file.video.name.clone(),
                        path,
                        titles,
                        year: m.year.clone(),
                        season_number: m.season_number,
                        episode_number: m.episode_number,
                        second_episode_number: m.second_episode_number,
                        resolution: m.resolution.clone(),
                        quality: m.quality.clone(),
                        video_codec: m.video_codec.clone(),
                        audio_codec: m.audio_codec.clone(),
                        hdr: m.hdr.clone(),
                        tmdb_id: Some(tv.id),
                        tmdb_title: Some(tv.name),
                        tmdb_year: tv.first_air_date.get(..4).map(|s| s.to_string()),
                        subtitle_files,
                    }),
                    None => results.push(ParsedMediaInfo::Unmatched {
                        file_name: file.video.name.clone(),
                        path,
                        titles,
                        year: m.year.clone(),
                    }),
                }
            } else {
                let movie_info = self
                    .tmdb_lookup
                    .get_movie_info(m, &file.descriptions)
                    .await?;
                match movie_info {
                    Some(movie) => results.push(ParsedMediaInfo::Movie {
                        file_name: file.video.name.clone(),
                        path,
                        titles,
                        year: m.year.clone(),
                        resolution: m.resolution.clone(),
                        quality: m.quality.clone(),
                        video_codec: m.video_codec.clone(),
                        audio_codec: m.audio_codec.clone(),
                        hdr: m.hdr.clone(),
                        tmdb_id: Some(movie.id),
                        tmdb_title: Some(movie.title),
                        tmdb_year: movie.release_date.get(..4).map(|s| s.to_string()),
                        subtitle_files,
                    }),
                    None => results.push(ParsedMediaInfo::Unmatched {
                        file_name: file.video.name.clone(),
                        path,
                        titles,
                        year: m.year.clone(),
                    }),
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
#[path = "parse/tests.rs"]
mod tests;
