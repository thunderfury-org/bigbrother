use std::collections::{BTreeMap, HashMap};

use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    media::Metadata,
};

#[derive(Debug, Clone)]
pub(super) struct RawFile {
    pub id: Option<i64>,
    pub name: String,
    pub etag: String,
    pub size: u64,
    pub path: String,
}

/// 表示一个媒体文件，包含视频文件和字幕文件
#[derive(Debug)]
pub(super) struct MediaFile {
    pub metadata: Box<Metadata>,
    pub video: RawFile,
    pub subtitles: Vec<RawFile>,
}

pub(super) enum Media<'a> {
    Movie {
        detail: MovieDetail,
        files: Vec<&'a MediaFile>,
    },
    Tv {
        detail: TvDetail,
        /// (season, episode) -> files[]
        files: BTreeMap<u32, BTreeMap<u32, Vec<&'a MediaFile>>>,
    },
}

pub(super) struct TransferEpisodeArgs<'a> {
    pub detail: &'a TvDetail,
    pub season_number: u32,
    pub episode_number: u32,
    pub files: &'a [&'a MediaFile],
    pub season_full_path: &'a str,
    pub season_dir_id: i64,
    pub existing_episode_files: &'a HashMap<u32, Vec<MediaFile>>,
}
