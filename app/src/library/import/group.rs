use std::collections::{BTreeMap, HashMap};

use tracing::info;

use super::{
    Importer,
    inner::{Media, MediaFile, RawFile},
};
use crate::{error::AppResult, media::Metadata};

/// 对原始文件进行分组，将视频文件和对应的字幕文件存储在同一 MediaFile 中
pub(super) fn group_video_and_subtitle_files(
    raw_files: Vec<(Box<Metadata>, RawFile)>,
) -> Vec<MediaFile> {
    if raw_files.is_empty() {
        return Vec::new();
    }

    let (video_files, subtitle_files): (Vec<_>, Vec<_>) = raw_files
        .into_iter()
        .partition(|(metadata, _)| metadata.is_video());

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

impl Importer {
    /// 按 tmdb 信息分组媒体文件，分类为 TV 和 Movie
    pub(super) async fn group_media_files<'a>(
        &mut self,
        files: &'a [MediaFile],
    ) -> AppResult<Vec<Media<'a>>> {
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
                let entry = grouped_files
                    .entry(tv_info.id)
                    .or_insert_with(|| Media::Tv {
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
                let entry = grouped_files
                    .entry(movie_info.id)
                    .or_insert_with(|| Media::Movie {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{FileType, Metadata};

    // Helper function to create a test RawFile
    fn create_raw_file(name: &str) -> RawFile {
        RawFile {
            id: None,
            name: name.to_string(),
            etag: super::super::inner::Etag::Md5("test".to_string()),
            size: 1024,
            path: format!("/test/{}", name),
        }
    }

    // Helper function to create video metadata
    fn create_video_metadata() -> Box<Metadata> {
        Box::new(Metadata {
            file_type: FileType::Video,
            ..Default::default()
        })
    }

    // Helper function to create subtitle metadata
    fn create_subtitle_metadata() -> Box<Metadata> {
        Box::new(Metadata {
            file_type: FileType::Subtitle,
            ..Default::default()
        })
    }

    #[test]
    fn test_group_empty_files() {
        let raw_files = vec![];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_group_no_video_files() {
        let raw_files = vec![
            (create_subtitle_metadata(), create_raw_file("sub1.srt")),
            (create_subtitle_metadata(), create_raw_file("sub2.srt")),
        ];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_group_only_video_files_no_subtitles() {
        let raw_files = vec![
            (create_video_metadata(), create_raw_file("video1.mp4")),
            (create_video_metadata(), create_raw_file("video2.mkv")),
        ];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].subtitles.len(), 0);
        assert_eq!(result[1].subtitles.len(), 0);
    }

    #[test]
    fn test_group_single_video_with_matching_subtitle() {
        let raw_files = vec![
            (create_video_metadata(), create_raw_file("movie.mp4")),
            (create_subtitle_metadata(), create_raw_file("movie.srt")),
        ];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].video.name, "movie.mp4");
        assert_eq!(result[0].subtitles.len(), 1);
        assert_eq!(result[0].subtitles[0].name, "movie.srt");
    }

    #[test]
    fn test_group_video_with_multiple_subtitles() {
        let raw_files = vec![
            (create_video_metadata(), create_raw_file("movie.mp4")),
            (create_subtitle_metadata(), create_raw_file("movie.en.srt")),
            (create_subtitle_metadata(), create_raw_file("movie.zh.srt")),
        ];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].subtitles.len(), 2);
    }

    #[test]
    fn test_group_multiple_videos_with_matching_subtitles() {
        let raw_files = vec![
            (create_video_metadata(), create_raw_file("video1.mp4")),
            (create_video_metadata(), create_raw_file("video2.mp4")),
            (create_subtitle_metadata(), create_raw_file("video1.srt")),
            (create_subtitle_metadata(), create_raw_file("video2.srt")),
        ];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 2);

        // Find the media files by video name
        let video1_file = result
            .iter()
            .find(|f| f.video.name == "video1.mp4")
            .unwrap();
        let video2_file = result
            .iter()
            .find(|f| f.video.name == "video2.mp4")
            .unwrap();

        assert_eq!(video1_file.subtitles.len(), 1);
        assert_eq!(video1_file.subtitles[0].name, "video1.srt");
        assert_eq!(video2_file.subtitles.len(), 1);
        assert_eq!(video2_file.subtitles[0].name, "video2.srt");
    }

    #[test]
    fn test_group_video_without_matching_subtitle() {
        let raw_files = vec![
            (create_video_metadata(), create_raw_file("video1.mp4")),
            (create_subtitle_metadata(), create_raw_file("different.srt")),
        ];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].subtitles.len(), 0);
    }

    #[test]
    fn test_group_subtitle_matches_by_prefix() {
        let raw_files = vec![
            (create_video_metadata(), create_raw_file("movie.mp4")),
            (
                create_subtitle_metadata(),
                create_raw_file("movie.en.forced.srt"),
            ),
        ];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].subtitles.len(), 1);
    }

    #[test]
    fn test_group_video_without_extension() {
        let raw_files = vec![
            (create_video_metadata(), create_raw_file("videofile")),
            (create_subtitle_metadata(), create_raw_file("videofile.srt")),
        ];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].video.name, "videofile");
        assert_eq!(result[0].subtitles.len(), 1);
    }

    #[test]
    fn test_group_complex_scenario() {
        let raw_files = vec![
            (create_video_metadata(), create_raw_file("movie1.mp4")),
            (create_video_metadata(), create_raw_file("movie2.mkv")),
            (create_video_metadata(), create_raw_file("movie3.avi")),
            (create_subtitle_metadata(), create_raw_file("movie1.en.srt")),
            (create_subtitle_metadata(), create_raw_file("movie1.zh.srt")),
            (create_subtitle_metadata(), create_raw_file("movie2.srt")),
            (create_subtitle_metadata(), create_raw_file("unmatched.srt")),
        ];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 3);

        let movie1 = result
            .iter()
            .find(|f| f.video.name == "movie1.mp4")
            .unwrap();
        let movie2 = result
            .iter()
            .find(|f| f.video.name == "movie2.mkv")
            .unwrap();
        let movie3 = result
            .iter()
            .find(|f| f.video.name == "movie3.avi")
            .unwrap();

        assert_eq!(movie1.subtitles.len(), 2);
        assert_eq!(movie2.subtitles.len(), 1);
        assert_eq!(movie3.subtitles.len(), 0);
    }

    #[test]
    fn test_group_subtitle_matches_only_once() {
        // When a subtitle file name starts with a video's stem, it's only assigned to that video
        let raw_files = vec![
            (create_video_metadata(), create_raw_file("movie.mp4")),
            (create_subtitle_metadata(), create_raw_file("movie.srt")),
        ];
        let result = group_video_and_subtitle_files(raw_files);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].subtitles.len(), 1);
    }
}
