use serde::Deserialize;

mod normalize;
mod parser;

pub const LANGUAGE_CHINESE: &str = "zh";
pub const LANGUAGE_JAPANESE: &str = "jp";
pub const LANGUAGE_ENGLISH: &str = "en";

pub const LANGUAGE_CHINESE_SIMPLIFIED: &str = "zh-CN";
pub const LANGUAGE_CHINESE_TRADITIONAL: &str = "zh-TW";

pub const FILE_TYPE_VIDEO: &str = "video";
pub const FILE_TYPE_SUBTITLE: &str = "subtitle";

/// Represents a media title with language information
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Title {
    /// The title text
    pub title: String,

    /// Language code for the title (e.g., "en", "fr")
    pub language: String,
}

/// Contains metadata information about media files
#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "lowercase")]
pub struct Metadata {
    /// File type based on file extension
    pub file_type: String,

    /// File extension (e.g: .mkv, .mp4, .srt)
    pub extension: String,

    /// TMDB ID for the media
    pub tmdb_id: String,

    /// Movie or TV Show titles
    pub titles: Vec<Title>,

    /// Release year
    pub year: String,

    /// Season number for TV shows
    pub season_number: Option<u32>,

    /// Episode number for TV shows
    pub episode_number: Option<u32>,

    /// For episode like 01-02
    pub second_episode_number: Option<u32>,

    /// Video resolution (e.g: 2160p, 1080p, 720p)
    pub resolution: String,

    /// Frame rate (e.g: 24fps, 30fps, 60fps)
    pub frame_rate: String,

    /// Quality of the media (e.g: BluRay, WEB-DL)
    pub quality: String,

    /// HDR type (e.g: HDR10, HDR10+, DV, HLG)
    pub hdr: String,

    /// Video codec (e.g: H264, H265)
    pub video_codec: String,

    /// Audio codec (e.g: AAC, DTS)
    pub audio_codec: String,

    /// Release group name
    pub release_group: String,

    /// Subtitle language (e.g: en, fr, es)
    pub subtitles: Vec<String>,
}

impl From<&str> for Metadata {
    fn from(value: &str) -> Self {
        parser::parse(value)
    }
}
