use std::collections::{BTreeMap, HashMap};

use tracing::info;

use super::{
    Importer,
    inner::{Media, MediaFile, RawFile},
};
use crate::{error::AppResult, media::Metadata};

impl Importer {
    /// 对原始文件进行分组，将视频文件和对应的字幕文件存储在同一 MediaFile 中
    pub(super) fn group_video_and_subtitle_files(&self, raw_files: Vec<(Box<Metadata>, RawFile)>) -> Vec<MediaFile> {
        if raw_files.is_empty() {
            return Vec::new();
        }

        let (video_files, subtitle_files): (Vec<_>, Vec<_>) =
            raw_files.into_iter().partition(|(metadata, _)| metadata.is_video());

        if video_files.is_empty() {
            // 没有视频文件
            return Vec::new();
        } else if subtitle_files.is_empty() {
            // 没有字幕文件
            return video_files
                .into_iter()
                .map(|(metadata, raw_file)| MediaFile {
                    metadata,
                    video: raw_file,
                    subtitles: Vec::new(),
                })
                .collect();
        }

        // 有多个视频文件，按照视频文件名（移除扩展名后）匹配字幕文件
        let mut media_files_map: HashMap<String, MediaFile> = video_files
            .into_iter()
            .map(|(metadata, raw_file)| {
                let file_stem = match raw_file.name.rfind('.') {
                    Some(i) => raw_file.name[..i].to_owned(),
                    None => raw_file.name.to_owned(),
                };
                (
                    file_stem,
                    MediaFile {
                        metadata,
                        video: raw_file,
                        subtitles: Vec::new(),
                    },
                )
            })
            .collect();

        for (_, subtitle_file) in subtitle_files {
            for (file_stem, media_file) in &mut media_files_map {
                if subtitle_file.name.starts_with(file_stem) {
                    media_file.subtitles.push(subtitle_file);
                    break;
                }
            }
        }

        media_files_map.into_values().collect()
    }

    /// 按 tmdb 信息分组媒体文件，分类为 TV 和 Movie
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

    /// 按 tmdb_id 分组 TV 文件
    async fn group_tv_file<'a>(
        &mut self,
        file: &'a MediaFile,
        grouped_files: &mut HashMap<u32, Media<'a>>,
    ) -> AppResult<()> {
        // 从 tmdb 获取 tv 详情
        let tv_info = self.get_tv_info_from_tmdb(&file.metadata).await?;
        match tv_info {
            Some(tv_info) => {
                let season_number = match file.metadata.season_number {
                    Some(s) => s,
                    None => {
                        if tv_info.number_of_seasons == 1 {
                            1
                        } else {
                            // multi season, but no season number found in file metadata
                            info!(
                                "Multi season tv, but no season number found in file: {}",
                                file.video.name
                            );

                            return Ok(());
                        }
                    }
                };
                let episode_number = match file.metadata.episode_number {
                    Some(e) => e,
                    None => {
                        // episode number not found in file metadata
                        info!("No episode number found in file: {}", file.video.name);

                        return Ok(());
                    }
                };
                let entry = grouped_files.entry(tv_info.id).or_insert_with(|| Media::Tv {
                    detail: tv_info,
                    files: BTreeMap::new(),
                });
                if let Media::Tv { files, .. } = entry {
                    files
                        .entry(season_number)
                        .or_insert_with(BTreeMap::new)
                        .entry(episode_number)
                        .or_insert_with(Vec::new)
                        .push(file);
                }
            }
            None => {
                info!(
                    "No tv found in tmdb for file: {}, path: {}",
                    file.video.name, file.video.path
                );
            }
        }

        Ok(())
    }

    /// 按 tmdb_id 分组 Movie 文件
    async fn group_movie_file<'a>(
        &mut self,
        file: &'a MediaFile,
        grouped_files: &mut HashMap<u32, Media<'a>>,
    ) -> AppResult<()> {
        // 从 tmdb 获取 movie 详情
        let movie_info = self.get_movie_info_from_tmdb(&file.metadata).await?;
        match movie_info {
            Some(movie_info) => {
                let entry = grouped_files.entry(movie_info.id).or_insert_with(|| Media::Movie {
                    detail: movie_info,
                    files: Vec::new(),
                });
                if let Media::Movie { files, .. } = entry {
                    files.push(file);
                }
            }
            None => {
                // movie not found in tmdb
                info!(
                    "No movie found in tmdb for file: {}, path: {}",
                    file.video.name, file.video.path
                );
            }
        }

        Ok(())
    }
}
