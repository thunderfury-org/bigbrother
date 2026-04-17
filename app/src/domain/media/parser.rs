use std::collections::HashSet;
use std::ops::Range;
use std::sync::LazyLock;

use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};
use regex::Regex;

use super::normalize::{
    normalize_audio_codec, normalize_hdr, normalize_language, normalize_quality,
    normalize_video_codec,
};
use super::{
    FileType, LANGUAGE_CHINESE_SIMPLIFIED, LANGUAGE_CHINESE_TRADITIONAL, LANGUAGE_ENGLISH,
    MediaKind, Metadata, Title,
};

static TMDB_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|[ ._\-\[\(\{])tmdb(?:id)?[-=](?P<value>\d+)").unwrap());
static FRAME_RATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?P<value>\d{2,3}fps)").unwrap());
static QUALITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?P<value>WEB-?DL|WEB-?Rip|WEBRIP|Blu-?Ray(?:[ ._-]?Remux)?|Remux|BR-?Rip|BD-?Rip)",
    )
    .unwrap()
});
static HDR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?P<value>HDR10\+?|HDR|Dolby[ -]?Vision|HLG|DV|DoVi)").unwrap()
});
static VIDEO_CODEC_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?P<value>[HX]\.?26[45]|AVC|HEVC|AV1|VP-9)").unwrap());
static AUDIO_CODEC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?P<value>(?:AAC|FLAC|Dolby[.\s]?Digital|DDP?|DTS(?:[.\s-]?HD)?|TrueHD)(?:[.\s_-]?(?:Atmos|MA|DDP?|\d\.\d))*)",
    )
    .unwrap()
});
static RESOLUTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?P<value>4k|\d{1,4}[pk]|\d{3,4}x(?P<height>\d{3,4}))").unwrap()
});
static YEAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?P<year>19\d{2}|20\d{2})").unwrap());
static SEASON_EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        (?:
            S(?P<season>\d{1,2})\s*[-._ ]?E(?P<episode>\d{1,4})(?:\s*[-~]\s*(?P<episode2>\d{1,4}))?
            |
            (?:Season|S)\s*(?P<season_alt>\d{1,2}).{0,8}?\[(?P<episode_alt>\d{1,4})(?:-(?P<episode_alt2>\d{1,4}))?\]
            |
            第\s*(?P<season_cn>\d{1,2})\s*季.{0,8}?\[(?P<episode_cn>\d{1,4})(?:-(?P<episode_cn2>\d{1,4}))?\]
        )",
    )
    .unwrap()
});
static CHINESE_EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"第\s*(?P<episode>\d{1,4})(?:\s*[-~]\s*(?P<episode2>\d{1,4}))?\s*集").unwrap()
});
static BRACKET_CHINESE_EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[(?:第\s*(?P<episode>\d{1,4})(?:\s*[-~]\s*(?P<episode2>\d{1,4}))?\s*集[^\]]*)\]")
        .unwrap()
});
static HASH_EPISODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#\s*(?P<episode>\d{1,4})").unwrap());
static EPISODE_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[ ._\-\[\(])E(?P<episode>\d{1,4})(?:$|[ ._\-\]\)])").unwrap()
});
static BRACKET_EPISODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(?P<episode>\d{1,4})(?:-(?P<episode2>\d{1,4}))?\]").unwrap());
static BRACKET_TITLE_EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[[^\]]*?[^\d\[\]]+\s+(?P<episode>\d{1,4})(?:\s+(?P<episode2>\d{1,4}))?[^\]]*\]")
        .unwrap()
});
static DASH_EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:^|[^\p{L}\p{N}])-\s*(?P<episode>\d{1,4})(?:\s*[-~]\s*(?P<episode2>\d{1,4}))?(?:$|[^\p{L}\p{N}])",
    )
    .unwrap()
});
static SEASON_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?ix)(?:第\s*(?P<season_cn>\d{1,2})\s*季|\b(?:Season|S)\s*(?P<season>\d{1,2})\b)")
        .unwrap()
});
static LEADING_GROUP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\[(?P<value>[^\[\]]+)\]").unwrap());
static BRACKET_CONTENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(?P<value>[^\[\]]+)\]").unwrap());
static PAREN_CONTENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\((?P<value>[^()]*)\)").unwrap());
static SUBTITLE_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[._-](?P<value>ja|en|chs|cht|zh-hans|zh-hant)$").unwrap());
static SUBTITLE_TOKEN_RE: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    vec![
        (
            LANGUAGE_CHINESE_SIMPLIFIED,
            Regex::new(
                r"(?ix)
                (?:
                    (^|[^a-z0-9])(?:chs|gb|zh-hans)([^a-z0-9]|$)
                    |简中|简体|簡中|簡體
                )
                ",
            )
            .unwrap(),
        ),
        (
            LANGUAGE_CHINESE_TRADITIONAL,
            Regex::new(
                r"(?ix)
                (?:
                    (^|[^a-z0-9])(?:cht|big5|zh-hant)([^a-z0-9]|$)
                    |繁中|繁体|繁體
                )
                ",
            )
            .unwrap(),
        ),
    ]
});
static AUDIO_NOISE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|[ ._\-\[\]\(\)])MA(?:$|[ ._\-\[\]\(\)])").unwrap());
static TITLE_REPLACE_RE: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"[\.\[\]\{\}\(\)]").unwrap(), " "),
        (Regex::new(r"第[^.\[\]]+季").unwrap(), ""),
        (Regex::new(r"\s+-\s+").unwrap(), " "),
    ]
});
static TITLE_TRIM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());
static DIGIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());
static TECHNICAL_FRAGMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        (?:WEB|WEB-?DL|WEB-?RIP|WEBRIP|BLU-?RAY|REMUX|BD-?RIP|BR-?RIP|HEVC|AVC|AV1|VP-9|H\.?26[45]|X26[45]
        |AAC|FLAC|DTS(?:-?HD)?|TRUEHD|ATMOS|HDR10\+?|HDR|DV|DOVI|HLG
        |\d{2,3}FPS|4K|\d{3,4}P|\d{3,4}X\d{3,4}|\d+(?:\.\d+)?G(?:B)?|[A-F0-9]{8}|MKV|MP4|SRT|ASS|SSA|PGS
        |CHS(?:[._-]?JP)?|CHT|BIG5|ZH-HANS|ZH-HANT|JP|简中|繁中|簡中|簡體|繁體|简体|繁体|简繁日多语|字幕|内封|外挂字幕
        |UHD|Ultra\s+HD|SDR|EXTENDED|P\d+|HQ|Baha|B-Global|ViuTV|附外挂字幕|招募翻译校对|日語原聲|日文自動產生字幕|进化版|Web先行版|先行版|英语中字|蓝光原盘)
        ",
    )
    .unwrap()
});
static NOISE_BRACKET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        ^(?:WEB|WEB-?DL|WEB-?RIP|WEBRIP|BLU-?RAY|REMUX|BD-?RIP|BR-?RIP|HEVC|AVC|AV1|VP-9|H\.?26[45]|X26[45]
        |AAC|FLAC|DTS(?:-?HD)?|TRUEHD|ATMOS|HDR10\+?|HDR|DV|DOVI|HLG
        |\d{2,3}FPS|4K|\d{3,4}P|\d{3,4}X\d{3,4}|\d+(?:\.\d+)?G(?:B)?|[A-F0-9]{8}|MKV|MP4|SRT|ASS|SSA|PGS
        |CHS(?:[._-]?JP)?|CHT|BIG5|ZH-HANS|ZH-HANT|JP|简中|繁中|簡中|簡體|繁體|简体|繁体|简繁内封|简繁日内封字幕|简日双语MP4/繁日双语MP4/简繁日多语MKV
        |字幕|内封|外挂字幕|附外挂字幕|招募翻译校对|日語原聲|日文自動產生字幕
        |UHD|Ultra\s+HD|SDR|EXTENDED|P\d+|HQ|Baha|B-Global|ViuTV)$",
    )
    .unwrap()
});
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

#[derive(Debug, Clone)]
struct EpisodeCandidate {
    season_number: Option<u32>,
    episode_number: u32,
    second_episode_number: Option<u32>,
    span: Range<usize>,
}

#[derive(Debug)]
struct ParsedMediaName {
    body: String,
    occupied: Vec<bool>,
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
            occupied: vec![false; body.len()],
            body,
            metadata,
            parsed_kind: ParsedMediaKind::Unknown,
            release_group_locked: false,
        }
    }

    fn parse(mut self) -> Box<Metadata> {
        self.parse_leading_group();
        self.parse_simple_value(
            &TMDB_RE,
            |meta, value| meta.tmdb_id = value.to_owned(),
            None,
        );
        self.parse_simple_value(
            &FRAME_RATE_RE,
            |meta, value| meta.frame_rate = value.to_lowercase(),
            None,
        );
        self.parse_simple_value(
            &QUALITY_RE,
            |meta, value| meta.quality = normalize_quality(value),
            None,
        );
        self.parse_hdr();
        self.parse_simple_value(
            &VIDEO_CODEC_RE,
            |meta, value| meta.video_codec = normalize_video_codec(value),
            None,
        );
        self.parse_simple_value(
            &AUDIO_CODEC_RE,
            |meta, value| meta.audio_codec = normalize_audio_codec(value),
            None,
        );
        self.parse_resolution();
        self.parse_year();
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

    fn parse_simple_value<F>(
        &mut self,
        re: &Regex,
        mut assign: F,
        predicate: Option<fn(&str) -> bool>,
    ) where
        F: FnMut(&mut Metadata, &str),
    {
        let mut selected = None;
        for caps in re.captures_iter(&self.body) {
            let Some(value_match) = caps.name("value") else {
                continue;
            };
            let value = value_match.as_str();
            if predicate.is_some_and(|check| !check(value)) {
                continue;
            }
            selected = Some((caps.get(0).unwrap().range(), value.to_owned()));
        }

        if let Some((span, value)) = selected {
            self.mark(span);
            assign(&mut self.metadata, value.as_str());
        }
    }

    fn parse_resolution(&mut self) {
        let mut selected = None;
        for caps in RESOLUTION_RE.captures_iter(&self.body) {
            if let Some(height_match) = caps.name("height") {
                selected = Some((
                    caps.get(0).unwrap().range(),
                    format!("{}p", height_match.as_str()),
                ));
            } else if let Some(resolution_match) = caps.name("value") {
                let mut resolution = resolution_match.as_str().to_lowercase();
                if resolution == "4k" {
                    resolution = "2160p".to_owned();
                }
                selected = Some((caps.get(0).unwrap().range(), resolution));
            }
        }

        if let Some((span, resolution)) = selected {
            self.mark(span);
            self.metadata.resolution = resolution;
        }
    }

    fn parse_hdr(&mut self) {
        let mut selected = None;
        let mut fallback = None;
        for caps in HDR_RE.captures_iter(&self.body) {
            let Some(value_match) = caps.name("value") else {
                continue;
            };
            let value = value_match.as_str();
            let span = caps.get(0).unwrap().range();
            let normalized = normalize_hdr(value);
            if normalized == "DV" {
                selected = Some((span, normalized));
            } else {
                fallback = Some((span, normalized));
            }
        }

        if let Some((span, hdr)) = selected.or(fallback) {
            self.mark(span);
            self.metadata.hdr = hdr;
        }
    }

    fn parse_year(&mut self) {
        let mut years = Vec::new();
        for caps in YEAR_RE.captures_iter(&self.body) {
            let Some(year_match) = caps.name("year") else {
                continue;
            };
            if !is_standalone_year(self.body.as_str(), &year_match.range()) {
                continue;
            }
            let year = year_match.as_str().parse::<u32>().unwrap_or_default();
            if !(1900..=2099).contains(&year) {
                continue;
            }
            years.push((year_match.range(), year_match.as_str().to_owned()));
        }

        if let Some((span, year)) = years.last() {
            self.mark(span.clone());
            self.metadata.year = year.clone();
        }
    }

    fn parse_episode(&mut self) {
        let mut candidate = self.find_episode_with_season();
        if candidate.is_none() {
            candidate = self.find_episode_without_season();
        }

        if let Some(candidate) = candidate {
            self.mark(candidate.span);
            self.metadata.season_number = candidate.season_number;
            self.metadata.episode_number = Some(candidate.episode_number);
            self.metadata.second_episode_number = candidate.second_episode_number;
            self.parsed_kind = ParsedMediaKind::TvEpisode;
            self.consume_redundant_episode_tags();
        }
    }

    fn find_episode_with_season(&self) -> Option<EpisodeCandidate> {
        let mut selected = None;
        for caps in SEASON_EPISODE_RE.captures_iter(&self.body) {
            let season_number = parse_u32(
                caps.name("season")
                    .or_else(|| caps.name("season_alt"))
                    .or_else(|| caps.name("season_cn"))?
                    .as_str(),
            )?;
            let episode_number = parse_u32(
                caps.name("episode")
                    .or_else(|| caps.name("episode_alt"))
                    .or_else(|| caps.name("episode_cn"))?
                    .as_str(),
            )?;
            let second_episode_number = caps
                .name("episode2")
                .or_else(|| caps.name("episode_alt2"))
                .or_else(|| caps.name("episode_cn2"))
                .and_then(|m| parse_u32(m.as_str()));

            selected = Some(EpisodeCandidate {
                season_number: Some(season_number),
                episode_number,
                second_episode_number,
                span: caps.get(0).unwrap().range(),
            });
        }

        selected
    }

    fn find_episode_without_season(&self) -> Option<EpisodeCandidate> {
        for re in [
            &BRACKET_CHINESE_EPISODE_RE,
            &CHINESE_EPISODE_RE,
            &EPISODE_PREFIX_RE,
            &HASH_EPISODE_RE,
            &BRACKET_EPISODE_RE,
            &DASH_EPISODE_RE,
        ] {
            let mut selected = None;
            for caps in re.captures_iter(&self.body) {
                let Some(episode_match) = caps.name("episode") else {
                    continue;
                };
                let episode_number = parse_u32(episode_match.as_str())?;
                if looks_like_year_episode(episode_number, &caps.get(0).unwrap().range()) {
                    continue;
                }

                let second_episode_number =
                    caps.name("episode2").and_then(|m| parse_u32(m.as_str()));
                selected = Some(EpisodeCandidate {
                    season_number: None,
                    episode_number,
                    second_episode_number,
                    span: caps.get(0).unwrap().range(),
                });
            }

            if selected.is_some() {
                return selected;
            }
        }

        let mut selected = None;
        for caps in BRACKET_TITLE_EPISODE_RE.captures_iter(&self.body) {
            let Some(episode_match) = caps.name("episode") else {
                continue;
            };
            let episode_number = parse_u32(episode_match.as_str())?;
            if looks_like_year_episode(episode_number, &episode_match.range()) {
                continue;
            }
            let episode_span = episode_match.start()..caps.get(0).unwrap().end();
            selected = Some(EpisodeCandidate {
                season_number: None,
                episode_number,
                second_episode_number: caps.name("episode2").and_then(|m| parse_u32(m.as_str())),
                span: episode_span,
            });
        }

        if selected.is_some() {
            return selected;
        }

        None
    }

    fn parse_season_only(&mut self) {
        if self.metadata.season_number.is_some() {
            return;
        }

        let mut selected = None;
        for caps in SEASON_ONLY_RE.captures_iter(&self.body) {
            let value = caps
                .name("season")
                .or_else(|| caps.name("season_cn"))
                .map(|m| m.as_str())
                .and_then(parse_u32);
            if let Some(season_number) = value {
                selected = Some((caps.get(0).unwrap().range(), season_number));
            }
        }

        if let Some((span, season_number)) = selected {
            self.mark(span);
            self.metadata.season_number = Some(season_number);
        }
    }

    fn consume_redundant_episode_tags(&mut self) {
        let spans = [
            &CHINESE_EPISODE_RE,
            &BRACKET_CHINESE_EPISODE_RE,
            &HASH_EPISODE_RE,
            &EPISODE_PREFIX_RE,
        ]
        .into_iter()
        .flat_map(|re| {
            re.find_iter(&self.body)
                .map(|m| m.range())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

        for span in spans {
            self.mark(span);
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
                && !is_noise_bracket(value)
                && (looks_like_release_group(value)
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
        if value.is_empty() || is_noise_bracket(value) || !looks_like_release_group(value) {
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
                if is_noise_bracket(value.as_str()) {
                    self.mark(span);
                    continue;
                }
                if looks_like_release_group(value.as_str())
                    && (span.start < title_boundary
                        || has_recent_occupied_before(&self.occupied, span.start, 32))
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

        if let Some((span, group)) = trailing_release_group(&self.body, &self.occupied) {
            self.metadata.release_group = group;
            self.release_group_locked = true;
            self.mark(span);
        }
    }

    fn parse_subtitles(&mut self) {
        let mut languages = Vec::new();
        let mut spans = Vec::new();

        for caps in BRACKET_CONTENT_RE.captures_iter(&self.body) {
            if let Some(value) = caps.name("value")
                && collect_subtitle_languages(value.as_str(), &mut languages)
            {
                spans.push(caps.get(0).unwrap().range());
            }
        }

        for caps in PAREN_CONTENT_RE.captures_iter(&self.body) {
            if let Some(value) = caps.name("value")
                && collect_subtitle_languages(value.as_str(), &mut languages)
            {
                spans.push(caps.get(0).unwrap().range());
            }
        }

        if let Some(caps) = SUBTITLE_SUFFIX_RE.captures(&self.body)
            && let Some(value) = caps.name("value")
            && collect_subtitle_languages(value.as_str(), &mut languages)
        {
            spans.push(caps.get(0).unwrap().range());
        }

        for span in spans {
            self.mark(span);
        }

        self.metadata.subtitles = languages;
    }

    fn parse_subtitle_suffix(&mut self) {
        if self.metadata.file_type != FileType::Subtitle {
            return;
        }

        if let Some(caps) = SUBTITLE_SUFFIX_RE.captures(&self.body) {
            self.mark(caps.get(0).unwrap().range());
        }
    }

    fn consume_noise_segments(&mut self) {
        let paren_spans = PAREN_CONTENT_RE
            .captures_iter(&self.body)
            .filter_map(|caps| {
                let value = caps.name("value")?.as_str().to_owned();
                let span = caps.get(0).unwrap().range();
                is_metadata_fragment(value.as_str()).then_some(span)
            })
            .collect::<Vec<_>>();
        for span in paren_spans {
            self.mark(span);
        }

        let bracket_spans = BRACKET_CONTENT_RE
            .captures_iter(&self.body)
            .filter_map(|caps| {
                let value = caps.name("value")?.as_str().to_owned();
                let span = caps.get(0).unwrap().range();
                is_noise_bracket(value.as_str()).then_some(span)
            })
            .collect::<Vec<_>>();
        for span in bracket_spans {
            self.mark(span);
        }

        let technical_spans = TECHNICAL_FRAGMENT_RE
            .find_iter(&self.body)
            .map(|m| m.range())
            .collect::<Vec<_>>();
        for span in technical_spans {
            self.mark(span);
        }

        let audio_noise_spans = AUDIO_NOISE_RE
            .find_iter(&self.body)
            .map(|m| m.range())
            .collect::<Vec<_>>();
        for span in audio_noise_spans {
            self.mark(span);
        }
    }

    fn parse_title(&mut self) {
        let mut text = self.unoccupied_text();
        for (re, replace_to) in TITLE_REPLACE_RE.iter() {
            text = re.replace_all(&text, *replace_to).into_owned();
        }

        let mut titles = Vec::new();
        for part in text.split('/') {
            let cleaned = cleanup_title_part(part);
            if cleaned.is_empty() {
                continue;
            }

            if DIGIT_RE.is_match(cleaned.as_str()) {
                titles.push(Title {
                    language: LANGUAGE_ENGLISH.to_owned(),
                    title: cleaned,
                });
                continue;
            }

            for language in LANG_DETECTOR.detect_multiple_languages_of(cleaned.as_str()) {
                let piece = cleaned[language.start_index()..language.end_index()].trim();
                if piece.is_empty() {
                    continue;
                }

                let title = if language.language() == Language::Chinese {
                    piece.replace('-', "").trim().to_owned()
                } else {
                    piece.to_owned()
                };
                if title.is_empty() {
                    continue;
                }

                titles.push(Title {
                    language: normalize_language(language.language()),
                    title,
                });
            }
        }

        if !titles.is_empty() {
            self.metadata.titles = titles;
        }
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
        let end = span.end.min(self.occupied.len());
        for index in span.start.min(end)..end {
            self.occupied[index] = true;
        }
    }

    fn unoccupied_text(&self) -> String {
        let mut text = String::with_capacity(self.body.len());
        for (index, ch) in self.body.char_indices() {
            let end = index + ch.len_utf8();
            if self.occupied[index..end].iter().any(|used| *used) {
                text.push(' ');
            } else {
                text.push(ch);
            }
        }
        text
    }

    fn first_occupied_index(&self) -> Option<usize> {
        self.occupied.iter().position(|used| *used)
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

fn parse_u32(value: &str) -> Option<u32> {
    value.trim().parse().ok()
}

fn cleanup_title_part(part: &str) -> String {
    let cleaned = part
        .trim_matches(|ch: char| matches!(ch, '.' | '-' | '_' | ' '))
        .trim()
        .to_owned();
    TITLE_TRIM_RE.replace_all(&cleaned, " ").trim().to_owned()
}

fn is_standalone_year(body: &str, span: &Range<usize>) -> bool {
    let prev = body[..span.start].chars().next_back();
    let next = body[span.end..].chars().next();

    is_year_boundary(prev) && is_year_boundary(next)
}

fn is_year_boundary(ch: Option<char>) -> bool {
    match ch {
        None => true,
        Some(value) => matches!(
            value,
            ' ' | '.' | '_' | '-' | '[' | ']' | '(' | ')' | '{' | '}'
        ),
    }
}

fn looks_like_year_episode(episode: u32, _span: &Range<usize>) -> bool {
    (1900..=2099).contains(&episode)
}

fn looks_like_release_group(value: &str) -> bool {
    if value.to_ascii_lowercase().contains("tmdb") {
        return false;
    }
    if value.contains('第') || value.contains('集') {
        return false;
    }
    if value.contains('/') {
        return false;
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '~' | ' '))
    {
        return false;
    }
    if value.is_empty() || is_noise_bracket(value) {
        return false;
    }

    let lower = value.to_ascii_lowercase();
    value.contains('-')
        || value.contains('&')
        || value.contains("字幕组")
        || value.contains("Raws")
        || value.contains("raws")
        || value.contains("Asia")
        || value.contains("个人翻译")
        || value.contains("制作")
        || (value.contains(' ')
            && (lower == value
                || lower.contains("raws")
                || lower.contains("sub")
                || lower.contains("team")
                || lower.contains("group")))
        || (value.len() <= 20
            && value.chars().any(|ch| ch.is_ascii_alphabetic())
            && !value.contains(' '))
}

fn trailing_release_group(body: &str, occupied: &[bool]) -> Option<(Range<usize>, String)> {
    let hyphen_index = body.rfind('-')?;
    let candidate = body[hyphen_index + 1..].trim();
    if candidate.is_empty() || !is_release_group_candidate(candidate) {
        return None;
    }
    if !has_trailing_technical_context(occupied, hyphen_index) {
        return None;
    }
    let prefix_token = body[..hyphen_index]
        .rsplit(['.', ' ', '_', '-', '[', ']', '(', ')'])
        .next()
        .unwrap_or_default()
        .trim();
    if prefix_token.eq_ignore_ascii_case("dolby") && candidate.eq_ignore_ascii_case("vision") {
        return None;
    }

    Some((hyphen_index..body.len(), candidate.to_owned()))
}

fn has_trailing_technical_context(occupied: &[bool], hyphen_index: usize) -> bool {
    has_recent_occupied_before(occupied, hyphen_index, 32)
}

fn has_recent_occupied_before(occupied: &[bool], index: usize, window: usize) -> bool {
    let window_start = index.saturating_sub(window);
    occupied
        .get(window_start..index)
        .is_some_and(|slice| slice.iter().any(|used| *used))
}

fn is_release_group_candidate(value: &str) -> bool {
    if value.to_ascii_lowercase().contains("tmdb") {
        return false;
    }
    if value.contains('第') || value.contains('集') {
        return false;
    }
    if value
        .chars()
        .any(|ch| matches!(ch, '[' | ']' | '{' | '}' | '/'))
    {
        return false;
    }
    if looks_like_technical_group(value) {
        return false;
    }
    if is_metadata_fragment(value) {
        return false;
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '~' | ' '))
    {
        return false;
    }
    if NOISE_BRACKET_RE.is_match(value) {
        return false;
    }

    !matches!(
        normalize_quality(value).as_str(),
        "WEB-DL" | "WEBRip" | "BluRay" | "Remux" | "BDRip" | "BRRip"
    )
}

fn looks_like_technical_group(value: &str) -> bool {
    let segments = value
        .split(|ch: char| ch == '.' || ch.is_whitespace())
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return false;
    }

    segments.iter().all(|segment| {
        let upper = segment.to_ascii_uppercase();
        matches!(
            upper.as_str(),
            "WEB"
                | "DL"
                | "DV"
                | "HDR"
                | "HDR10"
                | "HDR10+"
                | "AAC"
                | "DD"
                | "DDP"
                | "DTS"
                | "DTS-HD"
                | "HD"
                | "MA"
                | "TRUEHD"
                | "ATMOS"
                | "HEVC"
                | "AVC"
                | "H264"
                | "H265"
                | "REMUX"
                | "BLURAY"
                | "HQ"
        ) || upper.ends_with("FPS")
            || upper.starts_with("AAC")
            || upper.starts_with("DDP")
            || upper.starts_with("DTS")
            || upper.starts_with("TRUEHD")
            || segment.parse::<u32>().is_ok()
    })
}

fn is_noise_bracket(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return true;
    }
    if NOISE_BRACKET_RE.is_match(value) {
        return true;
    }
    if value.contains("字幕")
        && !value.contains("字幕组")
        && !value.contains("字幕社")
        && !value.contains("汉化组")
    {
        return true;
    }

    is_metadata_fragment(value)
}

fn is_metadata_fragment(value: &str) -> bool {
    let stripped = TECHNICAL_FRAGMENT_RE.replace_all(value, "");
    stripped
        .chars()
        .all(|ch| matches!(ch, ' ' | '.' | '-' | '_' | '/' | '@' | '&'))
}

fn prefix_contains_title(value: &str) -> bool {
    value
        .chars()
        .any(|ch| ch.is_alphabetic() || ('\u{4e00}'..='\u{9fff}').contains(&ch))
}

fn has_promotional_prefix(value: &str) -> bool {
    let trimmed = value.trim_start();
    let Some(rest) = trimmed.strip_prefix('.') else {
        return false;
    };

    matches!(rest.trim_start().chars().next(), Some('[' | '('))
}

fn collect_subtitle_languages(fragment: &str, output: &mut Vec<String>) -> bool {
    let fragment = fragment.trim();
    if fragment.is_empty() {
        return false;
    }

    let initial_len = output.len();
    if fragment.contains("简繁") || fragment.contains("簡繁") || fragment.contains("繁简") {
        push_language(output, LANGUAGE_CHINESE_SIMPLIFIED);
        push_language(output, LANGUAGE_CHINESE_TRADITIONAL);
    }

    for (language, pattern) in SUBTITLE_TOKEN_RE.iter() {
        if pattern.is_match(fragment) {
            push_language(output, language);
        }
    }

    output.len() != initial_len
}

fn push_language(output: &mut Vec<String>, language: &str) {
    if !output.iter().any(|existing| existing == language) {
        output.push(language.to_owned());
    }
}

pub fn parse(name: &str) -> Box<Metadata> {
    ParsedMediaName::new(name).parse()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use serde::Deserialize;

    use crate::domain::media::Metadata;

    #[derive(Deserialize)]
    struct TestCase {
        input: String,
        expected: Metadata,
    }

    #[test]
    fn test_parse_media() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("domain")
            .join("media")
            .join("testdata");

        let files = vec![
            "anime.yaml",
            "dir.yaml",
            "movie.yaml",
            "tv_episode.yaml",
            "tv_season_episode.yaml",
        ];

        for file in files {
            let content = fs::read_to_string(data_path.join(file)).unwrap();
            let cases: Vec<TestCase> = serde_yaml::from_str(&content).unwrap();
            for case in &cases {
                let info = super::parse(case.input.as_str());
                let mut expected = case.expected.clone();
                expected.media_kind = info.media_kind.clone();
                assert_eq!(expected, *info, "input: {}", case.input);
            }
        }
    }
}
