use serde::Deserialize;

mod normalize;
mod parser;

pub const LANGUAGE_CHINESE: &str = "zh";
pub const LANGUAGE_JAPANESE: &str = "jp";
pub const LANGUAGE_ENGLISH: &str = "en";

pub const LANGUAGE_CHINESE_SIMPLIFIED: &str = "zh-CN";
pub const LANGUAGE_CHINESE_TRADITIONAL: &str = "zh-TW";

/// Represents the type of media file (video or subtitle)
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    Video,
    Subtitle,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Represents a media title with language information
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Title {
    /// The title text
    pub title: String,

    /// Language code for the title (e.g., "en", "fr")
    pub language: String,
}

/// Contains metadata information about media files
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, rename_all = "lowercase")]
pub struct Metadata {
    /// File type based on file extension
    pub file_type: FileType,

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

impl Metadata {
    pub fn parse(value: &str) -> Box<Self> {
        parser::parse(value)
    }

    pub fn merge_metadata(&mut self, other: &Metadata) {
        // For titles, iterate through `other.titles` and only add a title if it doesn't already exist in `self.titles`.
        for other_title in &other.titles {
            if !self.titles.iter().any(|t| t.title == other_title.title) {
                self.titles.push(other_title.clone());
            }
        }

        // For year, always take `other.year` if it's not empty, overriding `self.year`.
        if !other.year.is_empty() {
            self.year = other.year.clone();
        }

        if self.tmdb_id.is_empty() && !other.tmdb_id.is_empty() {
            self.tmdb_id = other.tmdb_id.clone();
        }
        if self.season_number.is_none() && other.season_number.is_some() {
            self.season_number = other.season_number;
        }
        if self.second_episode_number.is_none() && other.second_episode_number.is_some() {
            self.second_episode_number = other.second_episode_number;
        }
        if self.resolution.is_empty() && !other.resolution.is_empty() {
            self.resolution = other.resolution.clone();
        }
        if self.frame_rate.is_empty() && !other.frame_rate.is_empty() {
            self.frame_rate = other.frame_rate.clone();
        }
        if self.quality.is_empty() && !other.quality.is_empty() {
            self.quality = other.quality.clone();
        }
        if self.hdr.is_empty() && !other.hdr.is_empty() {
            self.hdr = other.hdr.clone();
        }
        if self.video_codec.is_empty() && !other.video_codec.is_empty() {
            self.video_codec = other.video_codec.clone();
        }
        if self.audio_codec.is_empty() && !other.audio_codec.is_empty() {
            self.audio_codec = other.audio_codec.clone();
        }
        if self.release_group.is_empty() && !other.release_group.is_empty() {
            self.release_group = other.release_group.clone();
        }
        if self.subtitles.is_empty() && !other.subtitles.is_empty() {
            self.subtitles.extend(other.subtitles.clone());
        }
    }

    pub fn is_video(&self) -> bool {
        self.file_type == FileType::Video
    }

    pub fn unknown_type(&self) -> bool {
        self.file_type == FileType::Unknown
    }
}
