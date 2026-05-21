use std::collections::HashSet;
use std::ops::Range;
use std::sync::LazyLock;

use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};
use regex::Regex;

use super::super::{FileType, MediaKind, Metadata};
use super::extractors::{
    apply_basic_extractors_and_collect_spans, collect_noise_spans,
    extract_episode_and_collect_spans, extract_season_only_with_span, extract_subtitle_suffix_span,
    extract_subtitles_and_collect_spans,
};
use super::release_group::{
    is_metadata_fragment as is_metadata_fragment_v2, is_noise_bracket as is_noise_bracket_v2,
    looks_like_release_group as looks_like_release_group_v2,
    trailing_release_group as trailing_release_group_v2,
};
use super::span_mask::SpanMask;
use super::title_resolver::{
    prefix_contains_title, rebalance_title_boundaries, titles_from_cleaned_part,
};

static COMPACT_SEASON_EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)S(?P<season>\d{1,2})\s*[-._ ]?E(?P<episode>\d{1,4})(?:\s*[-~]\s*(?P<episode2>\d{1,4}))?",
    )
    .unwrap()
});
static LEADING_GROUP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\[(?P<value>[^\[\]]+)\]").unwrap());
static BRACKET_CONTENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(?P<value>[^\[\]]+)\]").unwrap());
static TITLE_REPLACE_RE: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"[\.\[\]\{\}\(\)]").unwrap(), " "),
        (Regex::new(r"第[^.\[\]]+季").unwrap(), ""),
        (Regex::new(r"\s+-\s+").unwrap(), " "),
    ]
});
static TITLE_TRIM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());
static DIGIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());
static NAME_NORMALIZE_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"[_（）《》]").unwrap(),
        Regex::new(r"[\[★](\S{1,4}年)?\S{1,2}月新番[\]★]").unwrap(),
        Regex::new(r"(?i)\[\d+(\.\d+)G?\]").unwrap(),
        Regex::new(r"(?i)10-?bit").unwrap(),
    ]
});

static VIDEO_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let extensions = [
        ".3g2", ".3gp", ".3gp2", ".asf", ".avi", ".divx", ".flv", ".iso", ".m4v", ".mk2", ".mk3d",
        ".mka", ".mkv", ".mov", ".mp4", ".mp4a", ".mpeg", ".mpg", ".ogg", ".ogm", ".ogv", ".qt",
        ".ra", ".ram", ".rm", ".ts", ".m2ts", ".vob", ".wav", ".webm", ".wma", ".wmv",
    ];
    HashSet::from(extensions)
});
static SUBTITLE_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let extensions = [".srt", ".sub", ".idx", ".ass", ".ssa"];
    HashSet::from(extensions)
});

static LANG_DETECTOR: LazyLock<LanguageDetector> = LazyLock::new(|| {
    let languages = vec![Language::English, Language::Chinese, Language::Japanese];
    LanguageDetectorBuilder::from_languages(&languages).build()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParsedMediaKind {
    TvEpisode,
    Unknown,
}

#[derive(Debug)]
struct ParsedMediaName {
    body: String,
    occupied: SpanMask,
    metadata: Metadata,
    parsed_kind: ParsedMediaKind,
    release_group_locked: bool,
}

impl ParsedMediaName {
    fn new(name: &str) -> Self {
        let (body, file_type, extension) = normalize_name(name);
        let metadata = Metadata {
            file_type,
            extension,
            ..Default::default()
        };

        Self {
            occupied: SpanMask::new(&body),
            body,
            metadata,
            parsed_kind: ParsedMediaKind::Unknown,
            release_group_locked: false,
        }
    }

    fn parse(mut self) -> Box<Metadata> {
        self.parse_leading_group();
        for span in apply_basic_extractors_and_collect_spans(&self.body, &mut self.metadata) {
            self.mark(span);
        }
        self.parse_episode();
        self.parse_season_only();
        self.parse_release_group();
        self.parse_subtitles();
        self.parse_subtitle_suffix();
        self.consume_noise_segments();
        self.parse_title();
        self.resolve_kind();

        Box::new(self.metadata)
    }

    fn parse_episode(&mut self) {
        if let Some(extraction) = extract_episode_and_collect_spans(&self.body) {
            for span in extraction.spans {
                self.mark(span);
            }
            self.metadata.season_number = extraction.season_number;
            self.metadata.episode_number = Some(extraction.episode_number);
            self.metadata.second_episode_number = extraction.second_episode_number;
            self.parsed_kind = ParsedMediaKind::TvEpisode;
        }
    }

    fn parse_season_only(&mut self) {
        if self.metadata.season_number.is_some() {
            return;
        }

        if let Some((span, season_number)) = extract_season_only_with_span(&self.body) {
            self.mark(span);
            self.metadata.season_number = Some(season_number);
        }
    }

    fn parse_leading_group(&mut self) {
        if let Some(caps) = LEADING_GROUP_RE.captures(&self.body)
            && let Some(value_match) = caps.name("value")
        {
            let value = value_match.as_str().trim();
            let raw_remainder = &self.body[caps.get(0).unwrap().end()..];
            let remainder = raw_remainder.trim();
            if !value.is_empty()
                && !is_noise_bracket_v2(value)
                && (looks_like_release_group_v2(value, is_noise_bracket_v2)
                    || raw_remainder.trim_start().starts_with('★')
                    || has_promotional_prefix(raw_remainder))
                && prefix_contains_title(remainder)
            {
                self.metadata.release_group = value.to_owned();
                self.release_group_locked = true;
                self.mark(caps.get(0).unwrap().range());
                return;
            }
        }

        let Some(caps) = BRACKET_CONTENT_RE.captures(&self.body) else {
            return;
        };
        if caps.get(0).unwrap().start() > 2 {
            return;
        }
        let Some(value_match) = caps.name("value") else {
            return;
        };
        let value = value_match.as_str().trim();
        if value.is_empty()
            || is_noise_bracket_v2(value)
            || !looks_like_release_group_v2(value, is_noise_bracket_v2)
        {
            return;
        }

        self.metadata.release_group = value.to_owned();
        self.release_group_locked = true;
        self.mark(caps.get(0).unwrap().range());
    }

    fn parse_release_group(&mut self) {
        let title_boundary = self.first_occupied_index().unwrap_or(self.body.len());
        if !self.release_group_locked {
            let captures = BRACKET_CONTENT_RE
                .captures_iter(&self.body)
                .filter_map(|caps| {
                    let value = caps.name("value")?.as_str().trim().to_owned();
                    Some((caps.get(0).unwrap().range(), value))
                })
                .collect::<Vec<_>>();
            for (span, value) in captures {
                if is_noise_bracket_v2(value.as_str()) {
                    self.mark(span);
                    continue;
                }
                if looks_like_release_group_v2(value.as_str(), is_noise_bracket_v2)
                    && (span.start < title_boundary
                        || self.occupied.has_recent_occupied_before(span.start, 32))
                {
                    self.metadata.release_group = value;
                    self.release_group_locked = true;
                    self.mark(span);
                }
            }
        }

        if self.release_group_locked {
            return;
        }

        if let Some((span, group)) = trailing_release_group_v2(
            &self.body,
            self.occupied.as_slice(),
            is_metadata_fragment_v2,
            is_noise_bracket_v2,
        ) {
            self.metadata.release_group = group;
            self.release_group_locked = true;
            self.mark(span);
        }
    }

    fn parse_subtitles(&mut self) {
        let extraction = extract_subtitles_and_collect_spans(&self.body);
        for span in extraction.spans {
            self.mark(span);
        }
        self.metadata.subtitles = extraction.languages;
    }

    fn parse_subtitle_suffix(&mut self) {
        if self.metadata.file_type != FileType::Subtitle {
            return;
        }

        if let Some(span) = extract_subtitle_suffix_span(&self.body) {
            self.mark(span);
        }
    }

    fn consume_noise_segments(&mut self) {
        for span in collect_noise_spans(&self.body) {
            self.mark(span);
        }
    }

    fn parse_title(&mut self) {
        // 纯数字文件名当作集数（如 "8.mp4" → episode 8）
        let remaining = self.title_candidate_text();
        let trimmed = remaining.trim();
        if let Some(ep) = (!trimmed.is_empty() && DIGIT_RE.is_match(trimmed))
            .then(|| trimmed.parse::<u32>().ok())
            .flatten()
        {
            self.metadata.episode_number = Some(ep);
            self.parsed_kind = ParsedMediaKind::TvEpisode;
            return;
        }

        let mut text = remaining;
        for (re, replace_to) in TITLE_REPLACE_RE.iter() {
            text = re.replace_all(&text, *replace_to).into_owned();
        }

        let mut titles = Vec::new();
        for part in text.split('/') {
            let cleaned = cleanup_title_part(part);
            if cleaned.is_empty() {
                continue;
            }
            titles.extend(titles_from_cleaned_part(cleaned.as_str(), &LANG_DETECTOR));
        }

        rebalance_title_boundaries(&mut titles);
        if !titles.is_empty() {
            self.metadata.titles = titles;
        }
    }

    fn title_candidate_text(&self) -> String {
        let remaining = self.unoccupied_text();
        let Some(boundary) = self.standard_scene_episode_boundary() else {
            return remaining;
        };

        remaining[..boundary].to_owned()
    }

    fn standard_scene_episode_boundary(&self) -> Option<usize> {
        if self.parsed_kind != ParsedMediaKind::TvEpisode {
            return None;
        }

        let caps = COMPACT_SEASON_EPISODE_RE.captures(&self.body)?;
        let span = caps.get(0)?.start();
        let prefix = self.body[..span]
            .trim_matches(|ch: char| matches!(ch, '.' | '-' | '_' | ' ' | '[' | ']' | '(' | ')'));
        if prefix.is_empty() || prefix.contains('/') {
            return None;
        }
        if !prefix_contains_title(prefix) {
            return None;
        }

        Some(span)
    }

    fn resolve_kind(&mut self) {
        self.metadata.media_kind = match self.parsed_kind {
            ParsedMediaKind::TvEpisode => MediaKind::TvEpisode,
            ParsedMediaKind::Unknown => {
                if self.metadata.file_type != FileType::Unknown || !self.metadata.titles.is_empty()
                {
                    MediaKind::Movie
                } else {
                    MediaKind::Unknown
                }
            }
        };
    }

    fn mark(&mut self, span: Range<usize>) {
        self.occupied.mark(span);
    }

    fn unoccupied_text(&self) -> String {
        self.occupied.unoccupied_text(&self.body)
    }

    fn first_occupied_index(&self) -> Option<usize> {
        self.occupied.first_occupied_index()
    }
}

fn normalize_name(name: &str) -> (String, FileType, String) {
    let (body, extension) = split_extension(name);
    let mut normalized = body.replace("【", "[");
    normalized = normalized.replace("】", "]");
    normalized = normalized.replace("精校", ".");
    for re in NAME_NORMALIZE_RE.iter() {
        normalized = re.replace_all(&normalized, ".").into_owned();
    }
    let normalized = normalized.trim().to_owned();

    let file_type = if VIDEO_EXTENSIONS.contains(extension.as_str()) {
        FileType::Video
    } else if SUBTITLE_EXTENSIONS.contains(extension.as_str()) {
        FileType::Subtitle
    } else {
        FileType::Unknown
    };

    (normalized, file_type, extension)
}

fn split_extension(name: &str) -> (String, String) {
    if let Some(dot_index) = name.rfind('.') {
        let extension = name[dot_index..].trim().to_lowercase();
        if VIDEO_EXTENSIONS.contains(extension.as_str())
            || SUBTITLE_EXTENSIONS.contains(extension.as_str())
        {
            return (name[..dot_index].to_owned(), extension);
        }
    }

    (name.to_owned(), String::new())
}

fn cleanup_title_part(part: &str) -> String {
    let cleaned = part
        .trim_matches(|ch: char| matches!(ch, '.' | '-' | '_' | ' '))
        .trim()
        .to_owned();
    TITLE_TRIM_RE.replace_all(&cleaned, " ").trim().to_owned()
}

fn has_promotional_prefix(value: &str) -> bool {
    let trimmed = value.trim_start();
    let Some(rest) = trimmed.strip_prefix('.') else {
        return false;
    };

    matches!(rest.trim_start().chars().next(), Some('[' | '('))
}

pub fn parse(name: &str) -> Box<Metadata> {
    ParsedMediaName::new(name).parse()
}
