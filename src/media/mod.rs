use serde::Deserialize;

mod normalize;
mod parser;

pub use parser::parse;

pub const LANGUAGE_CHINESE: &str = "zh";
pub const LANGUAGE_JAPANESE: &str = "jp";
pub const LANGUAGE_ENGLISH: &str = "en";

pub const LANGUAGE_CHINESE_SIMPLIFIED: &str = "zh-CN";
pub const LANGUAGE_CHINESE_TRADITIONAL: &str = "zh-TW";

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaFileType {
    Video,
    Subtitle,
}

/// Represents a media title with language information
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct MediaTitle {
    /// The title text
    pub title: String,

    /// Language code for the title (e.g., "en", "fr")
    pub language: String,
}

/// Contains metadata information about media files
#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct MediaInfo {
    /// File type based on file extension
    pub file_type: Option<MediaFileType>,

    /// File extension (e.g: .mkv, .mp4, .srt)
    pub extension: Option<String>,

    /// TMDB ID for the media
    pub tmdb_id: Option<String>,

    /// Movie or TV Show titles
    pub titles: Option<Vec<MediaTitle>>,

    /// Release year
    pub year: Option<String>,

    /// Season number for TV shows
    pub season_number: Option<u32>,

    /// Episode number for TV shows
    pub episode_number: Option<u32>,

    /// For episode like 01-02
    pub second_episode_number: Option<u32>,

    /// Video resolution (e.g: 2160p, 1080p, 720p)
    pub resolution: Option<String>,

    /// Frame rate (e.g: 24fps, 30fps, 60fps)
    pub frame_rate: Option<String>,

    /// Quality of the media (e.g: BluRay, WEB-DL)
    pub quality: Option<String>,

    /// HDR type (e.g: HDR10, HDR10+, DV, HLG)
    pub hdr: Option<String>,

    /// Video codec (e.g: H264, H265)
    pub video_codec: Option<String>,

    /// Audio codec (e.g: AAC, DTS)
    pub audio_codec: Option<String>,

    /// Release group name
    pub release_group: Option<String>,

    /// Subtitle language (e.g: en, fr, es)
    pub subtitles: Option<Vec<String>>,
}
