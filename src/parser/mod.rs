use std::collections::HashSet;

use serde::Deserialize;
use title::Title;

pub mod episode;
pub mod lang;
pub mod title;

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileType {
    #[default]
    Unknown,
    Video,
    Subtitle,
}

impl From<&str> for FileType {
    fn from(val: &str) -> Self {
        lazy_static::lazy_static! {
            static ref VIDEO_EXT: HashSet<&'static str> = HashSet::from([
                "3g2", "3gp", "3gp2", "asf", "avi", "divx", "flv", "iso",
                "m4v", "mk2", "mk3d", "mka", "mkv", "mov", "mp4", "mp4a",
                "mpeg", "mpg", "ogg", "ogm", "ogv", "qt", "ra", "ram",
                "rm", "ts", "m2ts", "vob", "wav", "webm", "wma", "wmv"
            ]);

            static ref SUBTITLE_EXT: HashSet<&'static str> = HashSet::from([
                "srt", "idx", "sub", "ssa", "ass"
            ]);
        }

        if VIDEO_EXT.contains(val) {
            FileType::Video
        } else if SUBTITLE_EXT.contains(val) {
            FileType::Subtitle
        } else {
            FileType::Unknown
        }
    }
}

#[derive(Debug, Default, Deserialize, PartialEq)]
pub struct EpisodeInfo {
    pub file_type: FileType,
    pub extension: Option<String>,
    pub titles: Option<Vec<Title>>,
    pub release_group: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub resolution: Option<String>,
    pub subtitles: Option<Vec<String>>,
}
