use regex::Regex;
use std::ops::Range;
use std::sync::LazyLock;

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
    Regex::new(
        r"(?ix)
        (?:
            第\s*(?P<season_cn>[\d一二三四五六七八九十两兩零〇]{1,3})\s*季
            |
            \b(?:Season|S)\s*(?P<season>\d{1,2})\b
        )",
    )
    .unwrap()
});

use super::super::normalize::{
    normalize_audio_codec, normalize_hdr, normalize_quality, normalize_video_codec,
};
use super::super::{LANGUAGE_CHINESE_SIMPLIFIED, LANGUAGE_CHINESE_TRADITIONAL, Metadata};

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
    LazyLock::new(|| Regex::new(r"(?i)(?P<value>[HX]\.?(?:26[45])|AVC|HEVC|AV1|VP-9)").unwrap());
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EpisodeExtraction {
    pub(crate) season_number: Option<u32>,
    pub(crate) episode_number: u32,
    pub(crate) second_episode_number: Option<u32>,
    pub(crate) spans: Vec<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubtitleExtraction {
    pub(crate) languages: Vec<String>,
    pub(crate) spans: Vec<Range<usize>>,
}

pub(crate) fn extract_episode_and_collect_spans(body: &str) -> Option<EpisodeExtraction> {
    let candidate = find_episode_with_season(body).or_else(|| find_episode_without_season(body))?;

    let mut spans = vec![candidate.span.clone()];
    spans.extend(redundant_episode_tag_spans(body));

    Some(EpisodeExtraction {
        season_number: candidate.season_number,
        episode_number: candidate.episode_number,
        second_episode_number: candidate.second_episode_number,
        spans,
    })
}

pub(crate) fn extract_season_only_with_span(body: &str) -> Option<(Range<usize>, u32)> {
    let mut selected = None;
    for caps in SEASON_ONLY_RE.captures_iter(body) {
        let value = caps
            .name("season")
            .or_else(|| caps.name("season_cn"))
            .map(|m| m.as_str())
            .and_then(parse_season_number);
        if let Some(season_number) = value {
            selected = Some((caps.get(0).unwrap().range(), season_number));
        }
    }

    selected
}

pub(crate) fn extract_subtitles_and_collect_spans(body: &str) -> SubtitleExtraction {
    let mut languages = Vec::new();
    let mut spans = Vec::new();

    for caps in BRACKET_CONTENT_RE.captures_iter(body) {
        if let Some(value) = caps.name("value")
            && collect_subtitle_languages(value.as_str(), &mut languages)
        {
            spans.push(caps.get(0).unwrap().range());
        }
    }

    for caps in PAREN_CONTENT_RE.captures_iter(body) {
        if let Some(value) = caps.name("value")
            && collect_subtitle_languages(value.as_str(), &mut languages)
        {
            spans.push(caps.get(0).unwrap().range());
        }
    }

    if let Some(caps) = SUBTITLE_SUFFIX_RE.captures(body)
        && let Some(value) = caps.name("value")
        && collect_subtitle_languages(value.as_str(), &mut languages)
    {
        spans.push(caps.get(0).unwrap().range());
    }

    SubtitleExtraction { languages, spans }
}

pub(crate) fn extract_subtitle_suffix_span(body: &str) -> Option<Range<usize>> {
    SUBTITLE_SUFFIX_RE
        .captures(body)?
        .get(0)
        .map(|value| value.range())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EpisodeCandidate {
    season_number: Option<u32>,
    episode_number: u32,
    second_episode_number: Option<u32>,
    span: Range<usize>,
}

pub(crate) fn apply_basic_extractors_and_collect_spans(
    body: &str,
    metadata: &mut Metadata,
) -> Vec<Range<usize>> {
    let mut spans = Vec::new();

    if metadata.tmdb_id.is_empty()
        && let Some((span, tmdb_id)) = extract_tmdb_id_with_span(body)
    {
        metadata.tmdb_id = tmdb_id;
        spans.push(span);
    }
    if metadata.frame_rate.is_empty()
        && let Some((span, frame_rate)) = extract_frame_rate_with_span(body)
    {
        metadata.frame_rate = frame_rate;
        spans.push(span);
    }
    if metadata.quality.is_empty()
        && let Some((span, quality)) = extract_quality_with_span(body)
    {
        metadata.quality = quality;
        spans.push(span);
    }
    if metadata.hdr.is_empty()
        && let Some((span, hdr)) = extract_hdr_with_span(body)
    {
        metadata.hdr = hdr;
        spans.push(span);
    }
    if metadata.video_codec.is_empty()
        && let Some((span, video_codec)) = extract_video_codec_with_span(body)
    {
        metadata.video_codec = video_codec;
        spans.push(span);
    }
    if metadata.audio_codec.is_empty()
        && let Some((span, audio_codec)) = extract_audio_codec_with_span(body)
    {
        metadata.audio_codec = audio_codec;
        spans.push(span);
    }
    if metadata.resolution.is_empty()
        && let Some((span, resolution)) = extract_resolution_with_span(body)
    {
        metadata.resolution = resolution;
        spans.push(span);
    }
    if metadata.year.is_empty()
        && let Some((span, year)) = extract_year_with_span(body)
    {
        metadata.year = year;
        spans.push(span);
    }

    spans
}

fn extract_tmdb_id_with_span(body: &str) -> Option<(Range<usize>, String)> {
    TMDB_RE
        .captures_iter(body)
        .filter_map(|caps| {
            Some((
                caps.get(0)?.range(),
                caps.name("value")?.as_str().to_owned(),
            ))
        })
        .last()
}

fn extract_frame_rate_with_span(body: &str) -> Option<(Range<usize>, String)> {
    FRAME_RATE_RE
        .captures_iter(body)
        .filter_map(|caps| {
            Some((
                caps.get(0)?.range(),
                caps.name("value")?.as_str().to_lowercase(),
            ))
        })
        .last()
}

fn extract_quality_with_span(body: &str) -> Option<(Range<usize>, String)> {
    QUALITY_RE
        .captures_iter(body)
        .filter_map(|caps| {
            Some((
                caps.get(0)?.range(),
                normalize_quality(caps.name("value")?.as_str()),
            ))
        })
        .last()
}

fn extract_hdr_with_span(body: &str) -> Option<(Range<usize>, String)> {
    let mut selected = None;
    let mut fallback = None;

    for caps in HDR_RE.captures_iter(body) {
        let Some(value_match) = caps.name("value") else {
            continue;
        };
        let span = caps.get(0).unwrap().range();
        let normalized = normalize_hdr(value_match.as_str());
        if normalized == "DV" {
            selected = Some((span, normalized));
        } else {
            fallback = Some((span, normalized));
        }
    }

    selected.or(fallback)
}

fn extract_video_codec_with_span(body: &str) -> Option<(Range<usize>, String)> {
    VIDEO_CODEC_RE
        .captures_iter(body)
        .filter_map(|caps| {
            Some((
                caps.get(0)?.range(),
                normalize_video_codec(caps.name("value")?.as_str()),
            ))
        })
        .last()
}

fn extract_audio_codec_with_span(body: &str) -> Option<(Range<usize>, String)> {
    AUDIO_CODEC_RE
        .captures_iter(body)
        .filter_map(|caps| {
            Some((
                caps.get(0)?.range(),
                normalize_audio_codec(caps.name("value")?.as_str()),
            ))
        })
        .last()
}

fn extract_resolution_with_span(body: &str) -> Option<(Range<usize>, String)> {
    let mut selected = None;
    for caps in RESOLUTION_RE.captures_iter(body) {
        let span = caps.get(0).unwrap().range();
        if let Some(height_match) = caps.name("height") {
            selected = Some((span, format!("{}p", height_match.as_str())));
        } else if let Some(resolution_match) = caps.name("value") {
            let mut resolution = resolution_match.as_str().to_lowercase();
            if resolution == "4k" {
                resolution = "2160p".to_owned();
            }
            selected = Some((span, resolution));
        }
    }
    selected
}

fn extract_year_with_span(body: &str) -> Option<(Range<usize>, String)> {
    let mut years = Vec::new();
    for caps in YEAR_RE.captures_iter(body) {
        let Some(year_match) = caps.name("year") else {
            continue;
        };
        if !is_standalone_year(body, &year_match.range()) {
            continue;
        }
        let year = year_match.as_str().parse::<u32>().unwrap_or_default();
        if !(1900..=2099).contains(&year) {
            continue;
        }
        years.push((year_match.range(), year_match.as_str().to_owned()));
    }
    years.pop()
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

#[cfg(test)]
mod episode_tests {
    use super::*;

    #[test]
    fn extracts_scene_episode_and_marks_redundant_tags() {
        let extraction =
            extract_episode_and_collect_spans("Show.S01E02.第2集.1080p.WEB-DL").unwrap();

        assert_eq!(extraction.season_number, Some(1));
        assert_eq!(extraction.episode_number, 2);
        assert!(
            extraction
                .spans
                .iter()
                .any(|span| &"Show.S01E02.第2集.1080p.WEB-DL"[span.clone()] == "S01E02")
        );
        assert!(
            extraction
                .spans
                .iter()
                .any(|span| &"Show.S01E02.第2集.1080p.WEB-DL"[span.clone()] == "第2集")
        );
    }

    #[test]
    fn extracts_season_only_from_chinese_marker() {
        let (span, season_number) = extract_season_only_with_span("银河英雄传说 第三季").unwrap();

        assert_eq!(season_number, 3);
        assert_eq!(&"银河英雄传说 第三季"[span], "第三季");
    }

    #[test]
    fn extracts_subtitle_languages_and_spans() {
        let extraction = extract_subtitles_and_collect_spans("Show.[简繁内封].zh-Hans");

        assert_eq!(
            extraction.languages,
            vec![
                LANGUAGE_CHINESE_SIMPLIFIED.to_owned(),
                LANGUAGE_CHINESE_TRADITIONAL.to_owned(),
            ]
        );
        assert_eq!(extraction.spans.len(), 1);
    }

    #[test]
    fn extracts_subtitle_suffix_span_even_without_language_mapping() {
        let span = extract_subtitle_suffix_span("Show.ja").unwrap();

        assert_eq!(&"Show.ja"[span], ".ja");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_tmdb_id() {
        assert_eq!(
            extract_tmdb_id_with_span("Movie.tmdb-12345.1080p").map(|(_, value)| value),
            Some("12345".to_owned())
        );
    }

    #[test]
    fn extracts_frame_rate() {
        assert_eq!(
            extract_frame_rate_with_span("Movie.60fps.1080p").map(|(_, value)| value),
            Some("60fps".to_owned())
        );
    }

    #[test]
    fn extracts_quality() {
        assert_eq!(
            extract_quality_with_span("Movie.1080p.BluRay.Remux").map(|(_, value)| value),
            Some("Remux".to_owned())
        );
    }

    #[test]
    fn extracts_hdr() {
        assert_eq!(
            extract_hdr_with_span("Movie.2160p.HDR10+.DoVi").map(|(_, value)| value),
            Some("DV".to_owned())
        );
    }

    #[test]
    fn extracts_video_codec() {
        assert_eq!(
            extract_video_codec_with_span("Movie.H.264.AAC").map(|(_, value)| value),
            Some("H264".to_owned())
        );
    }

    #[test]
    fn extracts_audio_codec() {
        assert_eq!(
            extract_audio_codec_with_span("Movie.DTS-HD.MA5.1.H.264").map(|(_, value)| value),
            Some("DTS-HD.MA.5.1".to_owned())
        );
    }

    #[test]
    fn extracts_resolution() {
        assert_eq!(
            extract_resolution_with_span("Movie.4K.WEB-DL").map(|(_, value)| value),
            Some("2160p".to_owned())
        );
        assert_eq!(
            extract_resolution_with_span("Movie.1920x1080.WEB-DL").map(|(_, value)| value),
            Some("1080p".to_owned())
        );
    }

    #[test]
    fn extracts_year() {
        assert_eq!(
            extract_year_with_span("Movie.2024.1080p").map(|(_, value)| value),
            Some("2024".to_owned())
        );
    }

    #[test]
    fn basic_extractors_collect_spans() {
        let mut metadata = Metadata::default();
        let spans = apply_basic_extractors_and_collect_spans(
            "Movie.2024.1080p.WEB-DL.H.264.AAC",
            &mut metadata,
        );

        assert_eq!(metadata.year, "2024");
        assert_eq!(metadata.resolution, "1080p");
        assert_eq!(metadata.quality, "WEB-DL");
        assert_eq!(metadata.video_codec, "H264");
        assert_eq!(metadata.audio_codec, "AAC");
        assert!(!spans.is_empty());
    }
}

fn find_episode_with_season(body: &str) -> Option<EpisodeCandidate> {
    let mut selected = None;
    for caps in SEASON_EPISODE_RE.captures_iter(body) {
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

fn find_episode_without_season(body: &str) -> Option<EpisodeCandidate> {
    for re in [
        &BRACKET_CHINESE_EPISODE_RE,
        &CHINESE_EPISODE_RE,
        &EPISODE_PREFIX_RE,
        &HASH_EPISODE_RE,
        &BRACKET_EPISODE_RE,
        &DASH_EPISODE_RE,
    ] {
        let mut selected = None;
        for caps in re.captures_iter(body) {
            let Some(episode_match) = caps.name("episode") else {
                continue;
            };
            let episode_number = parse_u32(episode_match.as_str())?;
            if looks_like_year_episode(episode_number) {
                continue;
            }

            let second_episode_number = caps.name("episode2").and_then(|m| parse_u32(m.as_str()));
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
    for caps in BRACKET_TITLE_EPISODE_RE.captures_iter(body) {
        let Some(episode_match) = caps.name("episode") else {
            continue;
        };
        let episode_number = parse_u32(episode_match.as_str())?;
        if looks_like_year_episode(episode_number) {
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

    selected
}

fn redundant_episode_tag_spans(body: &str) -> Vec<Range<usize>> {
    [
        &CHINESE_EPISODE_RE,
        &BRACKET_CHINESE_EPISODE_RE,
        &HASH_EPISODE_RE,
        &EPISODE_PREFIX_RE,
    ]
    .into_iter()
    .flat_map(|re| re.find_iter(body).map(|m| m.range()).collect::<Vec<_>>())
    .collect()
}

fn parse_u32(value: &str) -> Option<u32> {
    value.trim().parse().ok()
}

fn parse_season_number(value: &str) -> Option<u32> {
    parse_u32(value).or_else(|| parse_chinese_number(value))
}

fn parse_chinese_number(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let mut total = 0;
    let mut current = 0;
    for ch in value.chars() {
        match ch {
            '零' | '〇' => {}
            '一' => current += 1,
            '二' | '两' | '兩' => current += 2,
            '三' => current += 3,
            '四' => current += 4,
            '五' => current += 5,
            '六' => current += 6,
            '七' => current += 7,
            '八' => current += 8,
            '九' => current += 9,
            '十' => {
                total += if current == 0 { 10 } else { current * 10 };
                current = 0;
            }
            _ => return None,
        }
    }

    let value = total + current;
    (value > 0).then_some(value)
}

fn looks_like_year_episode(episode: u32) -> bool {
    (1900..=2099).contains(&episode)
}
