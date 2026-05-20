use serde::Deserialize;

mod normalize;
mod parser;

pub const LANGUAGE_CHINESE: &str = "zh";
pub const LANGUAGE_JAPANESE: &str = "jp";
pub const LANGUAGE_ENGLISH: &str = "en";

pub const LANGUAGE_CHINESE_SIMPLIFIED: &str = "zh-CN";
pub const LANGUAGE_CHINESE_TRADITIONAL: &str = "zh-TW";

/// Represents the parsed semantic kind of a media name.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Movie,
    TvEpisode,
    #[default]
    Unknown,
}

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
    /// Semantic kind inferred from the media name
    pub media_kind: MediaKind,

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

    pub fn is_tv_episode(&self) -> bool {
        self.media_kind == MediaKind::TvEpisode
    }

    pub fn clear_tv_context(&mut self) {
        self.media_kind = MediaKind::Unknown;
        self.season_number = None;
        self.episode_number = None;
        self.second_episode_number = None;
    }

    pub fn unknown_type(&self) -> bool {
        self.file_type == FileType::Unknown
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::media::{
        FileType, LANGUAGE_CHINESE, LANGUAGE_CHINESE_SIMPLIFIED, LANGUAGE_ENGLISH, Metadata, Title,
    };

    #[test]
    fn parse_uses_domain_media_entrypoint() {
        let metadata = Metadata::parse("Movie.Title.2024.1080p.WEB-DL.H264.AAC.mkv");

        assert_eq!(metadata.file_type, FileType::Video);
        assert_eq!(metadata.extension, ".mkv");
        assert_eq!(metadata.year, "2024");
        assert_eq!(metadata.resolution, "1080p");
        assert_eq!(metadata.quality, "WEB-DL");
        assert_eq!(metadata.video_codec, "H264");
        assert_eq!(metadata.audio_codec, "AAC");
    }

    #[test]
    fn parse_tv_scene_title_excludes_source_tag_after_episode_marker() {
        let metadata = Metadata::parse("Perfect.Crown.S01E02.1080p.DSNP.WEB-DL.AAC.2.0.H.264.mkv");

        assert_eq!(metadata.file_type, FileType::Video);
        assert_eq!(metadata.season_number, Some(1));
        assert_eq!(metadata.episode_number, Some(2));
        assert!(
            metadata
                .titles
                .iter()
                .any(|title| title.title == "Perfect Crown")
        );
        assert!(
            metadata
                .titles
                .iter()
                .all(|title| !title.title.contains("DSNP"))
        );
    }

    #[test]
    fn parse_tv_scene_title_keeps_leading_release_group_separate() {
        let metadata =
            Metadata::parse("[Group] Perfect.Crown.S01E02.1080p.DSNP.WEB-DL.AAC.2.0.H.264.mkv");

        assert_eq!(metadata.release_group, "Group");
        assert!(
            metadata
                .titles
                .iter()
                .any(|title| title.title == "Perfect Crown")
        );
        assert!(
            metadata
                .titles
                .iter()
                .all(|title| !title.title.contains("DSNP"))
        );
    }

    #[test]
    fn parse_tv_scene_subtitle_excludes_source_tag_after_episode_marker() {
        let metadata = Metadata::parse("Perfect.Crown.S01E02.DSNP.zh-Hans.srt");

        assert_eq!(metadata.file_type, FileType::Subtitle);
        assert_eq!(metadata.season_number, Some(1));
        assert_eq!(metadata.episode_number, Some(2));
        assert_eq!(metadata.subtitles, vec![LANGUAGE_CHINESE_SIMPLIFIED]);
        assert!(
            metadata
                .titles
                .iter()
                .any(|title| title.title == "Perfect Crown")
        );
        assert!(
            metadata
                .titles
                .iter()
                .all(|title| !title.title.contains("DSNP"))
        );
    }

    #[test]
    fn merge_metadata_keeps_unique_titles_and_backfills_missing_fields() {
        let mut metadata = Metadata {
            titles: vec![Title {
                title: "Example".to_owned(),
                language: LANGUAGE_ENGLISH.to_owned(),
            }],
            tmdb_id: String::new(),
            quality: String::new(),
            subtitles: Vec::new(),
            ..Default::default()
        };
        let other = Metadata {
            titles: vec![
                Title {
                    title: "Example".to_owned(),
                    language: LANGUAGE_ENGLISH.to_owned(),
                },
                Title {
                    title: "示例".to_owned(),
                    language: LANGUAGE_CHINESE.to_owned(),
                },
            ],
            tmdb_id: "12345".to_owned(),
            year: "2025".to_owned(),
            quality: "WEB-DL".to_owned(),
            subtitles: vec![LANGUAGE_CHINESE_SIMPLIFIED.to_owned()],
            ..Default::default()
        };

        metadata.merge_metadata(&other);

        assert_eq!(metadata.titles.len(), 2);
        assert!(metadata.titles.iter().any(|title| title.title == "Example"));
        assert!(metadata.titles.iter().any(|title| title.title == "示例"));
        assert_eq!(metadata.tmdb_id, "12345");
        assert_eq!(metadata.year, "2025");
        assert_eq!(metadata.quality, "WEB-DL");
        assert_eq!(metadata.subtitles, vec![LANGUAGE_CHINESE_SIMPLIFIED]);
    }
}
