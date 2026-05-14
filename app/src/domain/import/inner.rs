use std::collections::{BTreeMap, HashMap};

use crate::domain::media::Metadata;
use crate::domain::share::RawFile;

use super::{MovieDetail, TvDetail};

/// 表示一个媒体文件，包含视频文件和字幕文件
#[derive(Debug)]
pub(crate) struct MediaFile {
    pub metadata: Box<Metadata>,
    pub video: RawFile,
    pub subtitles: Vec<RawFile>,
}

pub(crate) enum Media<'a> {
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

pub(crate) struct TransferEpisodeArgs<'a> {
    pub detail: &'a TvDetail,
    pub season_number: u32,
    pub episode_number: u32,
    pub files: &'a [&'a MediaFile],
    pub season_full_path: &'a str,
    pub season_dir_id: i64,
    pub existing_episode_files: &'a HashMap<u32, Vec<MediaFile>>,
}
